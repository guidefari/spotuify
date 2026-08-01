pub mod macos;

use gpui::{AppContext, Application, WindowOptions};
use spotuify_core::{Device, Playback, Queue};
use spotuify_launcher::{daemon_status, ensure_daemon_running, inspect_socket_state, SocketState};
use spotuify_protocol::{
    DaemonEvent, DaemonStatus, OperationSource, Request, Response, ResponseData, SearchScopeData,
    SearchSourceData,
};
use std::time::Duration;
use tokio::sync::{mpsc, mpsc::UnboundedReceiver, watch};

use crate::{
    icons::Assets,
    views::{DesktopApp, SearchRequest},
};

pub fn run() {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("spotuify-desktop-io")
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("failed to start spotuify desktop async runtime: {error}");
            return;
        }
    };
    let _runtime_guard = runtime.enter();

    Application::new().with_assets(Assets).run(move |app| {
        let _ = app.open_window(WindowOptions::default(), move |_, app_cx| {
            let view = app_cx.new(|_| DesktopApp::new());
            let view_for_task = view.clone();
            app_cx
                .spawn(move |cx: &mut gpui::AsyncApp| {
                    let cx = cx.clone();
                    async move {
                        bootstrap(view_for_task, cx).await;
                    }
                })
                .detach();
            view
        });
    });
}

async fn bootstrap(view: gpui::Entity<DesktopApp>, mut cx: gpui::AsyncApp) {
    let socket_path = spotuify_protocol::paths::socket_path();
    let socket_state = inspect_socket_state(&socket_path).await;

    if let Err(error) = ensure_daemon_running().await {
        let status = daemon_status()
            .await
            .unwrap_or_else(|_| fallback_status(&socket_path, socket_state, false));
        let message = error.to_string();
        let _ = view.update(&mut cx, |app, cx| {
            app.state = crate::views::DesktopState::Gate(crate::views::GateState {
                message,
                daemon_status: status,
                socket_state,
            });
            cx.notify();
        });
        return;
    }

    let mut client = match spotuify_protocol::IpcClient::connect().await {
        Ok(client) => client,
        Err(error) => {
            let status = daemon_status()
                .await
                .unwrap_or_else(|_| fallback_status(&socket_path, socket_state, false));
            let message = error.to_string();
            let _ = view.update(&mut cx, |app, cx| {
                app.state = crate::views::DesktopState::Gate(crate::views::GateState {
                    message,
                    daemon_status: status,
                    socket_state,
                });
                cx.notify();
            });
            return;
        }
    };

    let _ = client.subscribe_events().await;

    let command_client =
        match spotuify_protocol::IpcClient::connect_with_source(OperationSource::Agent).await {
            Ok(client) => Some(client),
            Err(error) => {
                let _ = view.update(&mut cx, |app, cx| {
                    app.toast = Some(format!("Transport unavailable: {error}"));
                    cx.notify();
                });
                None
            }
        };
    let (command_tx, command_rx) = mpsc::unbounded_channel();
    let (slider_tx, slider_rx) = watch::channel(None);
    let (search_tx, search_rx) = watch::channel(None);
    spawn_search_debouncer(search_rx, command_tx.clone());

    let doctor_report = fetch_doctor_report(&mut client).await;
    let seed = fetch_client_seed(&mut client).await;

    let status = daemon_status()
        .await
        .unwrap_or_else(|_| fallback_status(&socket_path, socket_state, true));
    let _ = view.update(&mut cx, |app, cx| {
        app.state = crate::views::DesktopState::Connected(crate::views::ConnectedState {
            daemon_status: status,
            doctor_report: doctor_report.map(Box::new),
            last_event: None,
        });
        app.set_command_senders(command_tx, slider_tx);
        app.set_search_sender(search_tx);
        if let Some((playback, queue, devices)) = seed {
            app.playback = Some(playback);
            app.set_queue_seed(queue);
            app.set_devices_seed(devices);
            app.request_artwork_for_current_track();
            app.request_current_track_membership();
        }
        app.toast = Some("Connected to daemon".to_string());
        cx.notify();
    });

    run_connected_loop(view, cx, client, command_client, command_rx, slider_rx).await;
}

fn spawn_search_debouncer(
    mut search_rx: watch::Receiver<Option<SearchRequest>>,
    command_tx: mpsc::UnboundedSender<Request>,
) {
    tokio::spawn(async move {
        loop {
            if search_rx.changed().await.is_err() {
                break;
            }

            let mut request = search_rx.borrow().clone();
            loop {
                match tokio::time::timeout(Duration::from_millis(250), search_rx.changed()).await {
                    Ok(Ok(())) => request = search_rx.borrow().clone(),
                    Ok(Err(_)) => return,
                    Err(_) => break,
                }
            }

            let Some(request) = request else {
                continue;
            };
            if command_tx
                .send(Request::SearchStream {
                    query: request.query,
                    scope: SearchScopeData::All,
                    source: SearchSourceData::legacy_default_remote(),
                    version: request.version,
                    provider: None,
                })
                .is_err()
            {
                return;
            }
        }
    });
}

async fn run_connected_loop(
    view: gpui::Entity<DesktopApp>,
    mut cx: gpui::AsyncApp,
    mut event_client: spotuify_protocol::IpcClient,
    mut command_client: Option<spotuify_protocol::IpcClient>,
    mut command_rx: UnboundedReceiver<Request>,
    mut slider_rx: watch::Receiver<Option<Request>>,
) {
    loop {
        tokio::select! {
            command = command_rx.recv() => {
                let Some(command) = command else { break };
                dispatch_transport_request(&view, &mut cx, &mut command_client, command).await;
            }
            changed = slider_rx.changed() => {
                if changed.is_err() {
                    break;
                }
                let command = slider_rx.borrow().clone();
                if let Some(command) = command {
                    dispatch_transport_request(
                        &view,
                        &mut cx,
                        &mut command_client,
                        command,
                    ).await;
                }
            }
            event = event_client.next_event() => {
                let Ok(event) = event else { break };
                let should_reseed = matches!(
                    &event,
                    DaemonEvent::EventStreamLagged { .. }
                        | DaemonEvent::PlaybackChanged { playback: None, .. }
                );

                let _ = view.update(&mut cx, |app, cx| {
                    app.apply_daemon_event(event);
                    cx.notify();
                });

                if should_reseed {
                    let seed = fetch_client_seed(&mut event_client).await;
                    let _ = view.update(&mut cx, |app, cx| {
                        if let Some((playback, queue, devices)) = seed {
                            app.playback = Some(playback);
                            app.set_queue_seed(queue);
                            app.set_devices_seed(devices);
                            app.request_artwork_for_current_track();
                            app.request_current_track_membership();
                        }
                        cx.notify();
                    });
                }
            }
        }
    }
}

async fn dispatch_transport_request(
    view: &gpui::Entity<DesktopApp>,
    cx: &mut gpui::AsyncApp,
    command_client: &mut Option<spotuify_protocol::IpcClient>,
    command: Request,
) {
    let Some(client) = command_client.as_mut() else {
        let _ = view.update(cx, |app, cx| {
            app.toast = Some("Transport is not connected to the daemon".to_string());
            cx.notify();
        });
        return;
    };

    let search_request = match &command {
        Request::SearchStream { query, version, .. } => Some((query.clone(), *version)),
        _ => None,
    };
    let lyrics_request = match &command {
        Request::LyricsGet {
            track_uri: Some(track_uri),
            ..
        } => Some(track_uri.clone()),
        _ => None,
    };
    let is_playlist_list = matches!(&command, Request::PlaylistsList { .. });
    let playlist_tracks_request = match &command {
        Request::PlaylistTracks { playlist, .. } => Some(playlist.clone()),
        _ => None,
    };
    let album_tracks_request = match &command {
        Request::AlbumTracks { album } => Some(album.clone()),
        _ => None,
    };
    let artist_albums_request = match &command {
        Request::ArtistAlbums { artist } => Some(artist.clone()),
        _ => None,
    };
    let is_queue_get = matches!(&command, Request::QueueGet);
    let artwork_request = match &command {
        Request::Image { url } | Request::CoverArt { url } => Some(url.clone()),
        _ => None,
    };
    let is_artwork_request = artwork_request.is_some();
    let is_devices_list = matches!(&command, Request::DevicesList);
    let is_saved_tracks = matches!(&command, Request::SavedTracks { .. });
    let is_library_list = matches!(&command, Request::LibraryList { .. });
    let is_followed_artists = matches!(&command, Request::FollowedArtists { .. });
    let is_recently_played = matches!(&command, Request::RecentlyPlayed { .. });
    let result = client.request(command).await;
    match result {
        Ok(Response::Ok { data }) => {
            let _ = view.update(cx, |app, cx| {
                if let Some(track_uri) = lyrics_request.as_deref() {
                    app.apply_daemon_response_for_track(data, Some(track_uri));
                } else if let Some(url) = artwork_request.as_deref() {
                    app.apply_artwork_response(url.to_string(), data);
                } else if let Some(playlist) = playlist_tracks_request.as_deref() {
                    app.apply_playlist_tracks_response(playlist, data);
                } else if let Some(album) = album_tracks_request.as_deref() {
                    app.apply_album_tracks_response(album, data);
                } else if let Some(artist) = artist_albums_request.as_deref() {
                    app.apply_artist_albums_response(artist, data);
                } else if is_recently_played {
                    app.apply_history_response(data);
                } else if is_library_list {
                    app.apply_albums_response(data);
                } else if is_followed_artists {
                    app.apply_artists_response(data);
                } else {
                    app.apply_daemon_response(data);
                }
                cx.notify();
            });
        }
        Ok(Response::Error { message, .. }) => {
            let _ = view.update(cx, |app, cx| {
                if let Some((query, version)) = &search_request {
                    app.fail_search(query, *version, message.clone());
                }
                if is_playlist_list {
                    app.fail_playlists(message.clone());
                }
                if let Some(playlist) = &playlist_tracks_request {
                    app.fail_playlist_tracks(playlist, message.clone());
                }
                if let Some(album) = &album_tracks_request {
                    app.fail_album_tracks(album, message.clone());
                }
                if let Some(artist) = &artist_albums_request {
                    app.fail_artist_albums(artist, message.clone());
                }
                if is_queue_get {
                    app.queue_loading = false;
                    app.queue_requested = false;
                }
                if let Some(url) = artwork_request.as_deref() {
                    app.fail_artwork(url);
                }
                if is_devices_list {
                    app.devices_loading = false;
                }
                if is_saved_tracks {
                    app.fail_liked_songs(message.clone());
                }
                if is_library_list {
                    app.fail_albums(message.clone());
                }
                if is_followed_artists {
                    app.fail_artists(message.clone());
                }
                if is_recently_played {
                    app.fail_history(message.clone());
                }
                if let Some(track_uri) = &lyrics_request {
                    app.fail_lyrics(track_uri, message.clone());
                }
                app.toast = Some(format!("Transport failed: {message}"));
                cx.notify();
            });
        }
        Err(error) => {
            let _ = view.update(cx, |app, cx| {
                if let Some((query, version)) = &search_request {
                    app.fail_search(query, *version, error.to_string());
                }
                if is_playlist_list {
                    app.fail_playlists(error.to_string());
                }
                if let Some(playlist) = &playlist_tracks_request {
                    app.fail_playlist_tracks(playlist, error.to_string());
                }
                if let Some(album) = &album_tracks_request {
                    app.fail_album_tracks(album, error.to_string());
                }
                if let Some(artist) = &artist_albums_request {
                    app.fail_artist_albums(artist, error.to_string());
                }
                if is_queue_get {
                    app.queue_loading = false;
                    app.queue_requested = false;
                }
                if let Some(url) = artwork_request.as_deref() {
                    app.fail_artwork(url);
                }
                if is_devices_list {
                    app.devices_loading = false;
                }
                if is_saved_tracks {
                    app.fail_liked_songs(error.to_string());
                }
                if is_library_list {
                    app.fail_albums(error.to_string());
                }
                if is_followed_artists {
                    app.fail_artists(error.to_string());
                }
                if is_recently_played {
                    app.fail_history(error.to_string());
                }
                if let Some(track_uri) = &lyrics_request {
                    app.fail_lyrics(track_uri, error.to_string());
                }
                if is_artwork_request {
                    if let Some(url) = artwork_request.as_deref() {
                        app.fail_artwork(url);
                    }
                }
                app.toast = Some(format!("Transport failed: {error}"));
                cx.notify();
            });
            *command_client =
                spotuify_protocol::IpcClient::connect_with_source(OperationSource::Agent)
                    .await
                    .ok();
        }
    }
}

async fn fetch_doctor_report(
    client: &mut spotuify_protocol::IpcClient,
) -> Option<spotuify_protocol::DoctorReport> {
    match client.request(Request::GetDoctorReport).await {
        Ok(Response::Ok {
            data: ResponseData::DoctorReport { report },
        }) => Some(report),
        _ => None,
    }
}

async fn fetch_client_seed(
    client: &mut spotuify_protocol::IpcClient,
) -> Option<(Playback, Queue, Vec<Device>)> {
    match client.request(Request::ClientSeed).await {
        Ok(Response::Ok {
            data:
                ResponseData::ClientSeed {
                    playback,
                    queue,
                    devices,
                    ..
                },
        }) => Some((playback, queue, devices)),
        _ => None,
    }
}

fn fallback_status(
    socket_path: &std::path::Path,
    socket_state: SocketState,
    running: bool,
) -> DaemonStatus {
    DaemonStatus {
        running,
        socket_path: socket_path.display().to_string(),
        socket_exists: socket_path.exists(),
        socket_reachable: running,
        stale_socket: socket_state == SocketState::Stale,
        daemon_pid: None,
        uptime_secs: None,
        protocol_version: spotuify_protocol::IPC_PROTOCOL_VERSION,
        daemon_version: None,
        daemon_build_id: None,
        audio_health: None,
    }
}
