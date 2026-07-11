use gpui::prelude::*;
use gpui::{
    div, fill, point, px, relative, rgb, App, Bounds, Context, CursorStyle, DragMoveEvent, Element,
    ElementId, ElementInputHandler, Entity, EntityInputHandler, FocusHandle, Focusable,
    GlobalElementId, IntoElement, KeyDownEvent, LayoutId, MouseButton, MouseDownEvent,
    MouseUpEvent, PaintQuad, Pixels, Point, Render, ShapedLine, SharedString, Style, TextRun,
    UTF16Selection, WeakEntity, Window,
};
use spotuify_core::{MediaItem, MediaKind, Playback, Playlist};
use spotuify_launcher::SocketState;
use spotuify_protocol::{
    DaemonEvent, DaemonStatus, DoctorReport, PlaybackCommand, Request, ResponseData,
    SearchScopeData, SearchSourceData, UpgradeHint,
};
use std::ops::Range;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc::UnboundedSender, watch};

pub struct DesktopApp {
    pub(crate) state: DesktopState,
    pub(crate) selected_destination: Destination,
    pub(crate) playback: Option<Playback>,
    pub(crate) update_banner: Option<UpdateBanner>,
    pub(crate) toast: Option<String>,
    pub(crate) command_tx: Option<UnboundedSender<Request>>,
    pub(crate) slider_tx: Option<watch::Sender<Option<Request>>>,
    search_tx: Option<watch::Sender<Option<SearchRequest>>>,
    search_query: String,
    search_results: Vec<MediaItem>,
    pub(crate) search_loading: bool,
    pub(crate) search_error: Option<String>,
    search_version: u64,
    search_playlists: Vec<Playlist>,
    playlist_picker_uri: Option<String>,
    pub(crate) playlist_loading: bool,
    search_input: Option<Entity<SearchInput>>,
    slider_drag: Option<SliderKind>,
    slider_preview: Option<SliderPreview>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SearchRequest {
    pub(crate) query: String,
    pub(crate) version: u64,
}

pub(crate) enum DesktopState {
    Booting,
    Gate(GateState),
    Connected(ConnectedState),
}

pub(crate) struct GateState {
    pub(crate) message: String,
    pub(crate) daemon_status: DaemonStatus,
    pub(crate) socket_state: SocketState,
}

pub(crate) struct ConnectedState {
    pub(crate) daemon_status: DaemonStatus,
    pub(crate) doctor_report: Option<Box<DoctorReport>>,
    pub(crate) last_event: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UpdateBanner {
    pub(crate) latest_version: String,
    pub(crate) release_url: Option<String>,
    pub(crate) command: Option<String>,
}

impl UpdateBanner {
    pub(crate) fn new(
        latest_version: String,
        release_url: Option<String>,
        upgrade: UpgradeHint,
    ) -> Self {
        let UpgradeHint { command, url, .. } = upgrade;

        Self {
            latest_version,
            release_url: release_url.or(url),
            command,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Destination {
    NowPlaying,
    Queue,
    Search,
    LikedSongs,
    Albums,
    Artists,
    Podcasts,
    Playlists,
    History,
    Notifications,
    Devices,
}

impl Destination {
    const ALL: [Self; 11] = [
        Self::NowPlaying,
        Self::Queue,
        Self::Search,
        Self::LikedSongs,
        Self::Albums,
        Self::Artists,
        Self::Podcasts,
        Self::Playlists,
        Self::History,
        Self::Notifications,
        Self::Devices,
    ];

    fn id(self) -> &'static str {
        match self {
            Self::NowPlaying => "now-playing",
            Self::Queue => "queue",
            Self::Search => "search",
            Self::LikedSongs => "liked-songs",
            Self::Albums => "albums",
            Self::Artists => "artists",
            Self::Podcasts => "podcasts",
            Self::Playlists => "playlists",
            Self::History => "history",
            Self::Notifications => "notifications",
            Self::Devices => "devices",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::NowPlaying => "Now Playing",
            Self::Queue => "Queue",
            Self::Search => "Search",
            Self::LikedSongs => "Liked Songs",
            Self::Albums => "Albums",
            Self::Artists => "Artists",
            Self::Podcasts => "Podcasts",
            Self::Playlists => "Playlists",
            Self::History => "History",
            Self::Notifications => "Notifications",
            Self::Devices => "Devices",
        }
    }

    fn stub(self) -> &'static str {
        match self {
            Self::NowPlaying => "Current playback details will expand here.",
            Self::Queue => "Queue management will bind to daemon queue events.",
            Self::Search => "Search tracks, artists, albums, playlists, and episodes.",
            Self::LikedSongs => "Liked songs will reuse the saved tracks daemon request.",
            Self::Albums => "Saved albums will land with the library panes.",
            Self::Artists => "Followed artists and discography browsing will appear here.",
            Self::Podcasts => "Podcast feeds will reuse the daemon episode feed.",
            Self::Playlists => "Playlist browsing gets wired after the shell.",
            Self::History => "Listening sessions and recent playback will appear here.",
            Self::Notifications => "Reminder and notification inbox state will appear here.",
            Self::Devices => "Device selection will bind to daemon devices state.",
        }
    }
}

impl DesktopApp {
    pub fn new() -> Self {
        Self {
            state: DesktopState::Booting,
            selected_destination: Destination::NowPlaying,
            playback: None,
            update_banner: None,
            toast: None,
            command_tx: None,
            slider_tx: None,
            search_tx: None,
            search_query: String::new(),
            search_results: Vec::new(),
            search_loading: false,
            search_error: None,
            search_version: 0,
            search_playlists: Vec::new(),
            playlist_picker_uri: None,
            playlist_loading: false,
            search_input: None,
            slider_drag: None,
            slider_preview: None,
        }
    }

    pub(crate) fn set_command_senders(
        &mut self,
        command_tx: UnboundedSender<Request>,
        slider_tx: watch::Sender<Option<Request>>,
    ) {
        self.command_tx = Some(command_tx);
        self.slider_tx = Some(slider_tx);
    }

    pub(crate) fn set_search_sender(&mut self, search_tx: watch::Sender<Option<SearchRequest>>) {
        self.search_tx = Some(search_tx);
    }

    fn send_request(&mut self, request: Request) {
        let Some(command_tx) = &self.command_tx else {
            self.toast = Some("Transport is not connected to the daemon".to_string());
            return;
        };

        if command_tx.send(request).is_err() {
            self.toast = Some("Transport connection closed".to_string());
        }
    }

    fn send_playback_command(&mut self, command: PlaybackCommand) {
        self.send_request(Request::PlaybackCommand { command });
    }

    fn send_slider_command(&mut self, command: PlaybackCommand) {
        let Some(slider_tx) = &self.slider_tx else {
            self.toast = Some("Transport is not connected to the daemon".to_string());
            return;
        };

        if slider_tx
            .send(Some(Request::PlaybackCommand { command }))
            .is_err()
        {
            self.toast = Some("Transport connection closed".to_string());
        }
    }

    fn update_search_query(&mut self, query: String) {
        if self.search_query != query {
            self.search_query = query;
            self.search_version = self.search_version.wrapping_add(1);
            self.search_results.clear();
            self.search_error = None;
            self.search_loading = !self.search_query.trim().is_empty();
            if let Some(search_tx) = &self.search_tx {
                let request = (!self.search_query.trim().is_empty()).then(|| SearchRequest {
                    query: self.search_query.trim().to_string(),
                    version: self.search_version,
                });
                let _ = search_tx.send(request);
            }
        }
    }

    fn start_search(&mut self) {
        let query = self.search_query.trim().to_string();
        if query.is_empty() {
            self.search_results.clear();
            self.search_error = None;
            self.search_loading = false;
            return;
        }

        if self.search_version == 0 {
            self.search_version = 1;
        }
        self.search_results.clear();
        self.search_error = None;
        self.search_loading = true;
        if let Some(search_tx) = &self.search_tx {
            let _ = search_tx.send(None);
        }
        self.send_request(Request::SearchStream {
            query,
            scope: SearchScopeData::All,
            source: SearchSourceData::Spotify,
            version: self.search_version,
        });
    }

    fn open_playlist_picker(&mut self, uri: String) {
        self.playlist_picker_uri = Some(uri);
        self.playlist_loading = true;
        self.search_playlists.clear();
        self.send_request(Request::PlaylistsList);
    }

    fn add_search_result_to_playlist(&mut self, playlist: String) {
        let Some(uri) = self.playlist_picker_uri.take() else {
            return;
        };
        self.playlist_loading = false;
        self.send_request(Request::PlaylistAddItems {
            playlist,
            uris: vec![uri],
        });
        self.toast = Some("Adding track to playlist".to_string());
    }

    pub(crate) fn apply_daemon_response(&mut self, response: ResponseData) {
        if let ResponseData::Playlists { playlists } = response {
            self.search_playlists = playlists;
            self.playlist_loading = false;
        }
    }

    pub(crate) fn fail_search(&mut self, query: &str, version: u64, message: String) {
        if version == self.search_version && query == self.search_query.trim() {
            self.search_loading = false;
            self.search_error = Some(message);
        }
    }

    fn preview_slider(&mut self, kind: SliderKind, preview: SliderPreview) {
        self.slider_drag = Some(kind);
        self.slider_preview = Some(preview);
    }

    fn finish_slider_drag(&mut self, kind: SliderKind) {
        match (kind, self.slider_preview) {
            (SliderKind::Seek, Some(SliderPreview::Seek(position_ms))) => {
                self.slider_preview = None;
                self.slider_drag = None;
                self.send_slider_command(PlaybackCommand::Seek { position_ms });
            }
            (SliderKind::Volume, Some(SliderPreview::Volume(volume_percent))) => {
                self.slider_preview = None;
                self.slider_drag = None;
                self.send_slider_command(PlaybackCommand::Volume { volume_percent });
            }
            _ => {}
        }
    }

    pub(crate) fn apply_daemon_event(&mut self, event: DaemonEvent) {
        let label = event_label(&event);

        if let DesktopState::Connected(state) = &mut self.state {
            state.last_event = Some(label);
        }

        match event {
            DaemonEvent::PlaybackChanged { action, playback } => {
                if let Some(playback) = playback {
                    self.playback = Some(playback);
                }
                if self.slider_drag.is_none() {
                    self.slider_preview = None;
                }
                if should_toast_playback_action(&action) {
                    self.toast = Some(format!("Playback updated: {action}"));
                }
            }
            DaemonEvent::SearchPage {
                query,
                version,
                items,
                ..
            } if version == self.search_version && query == self.search_query.trim() => {
                self.search_results.extend(items);
            }
            DaemonEvent::SearchComplete { query, version }
                if version == self.search_version && query == self.search_query.trim() =>
            {
                self.search_loading = false;
            }
            DaemonEvent::SearchFailed {
                query,
                version,
                message,
                ..
            } if version == self.search_version && query == self.search_query.trim() => {
                self.search_loading = false;
                self.search_error = Some(message);
            }
            DaemonEvent::MutationFinalized {
                status, message, ..
            } => match status {
                spotuify_protocol::ReceiptStatus::Failed => {
                    self.toast = Some(format!("Mutation failed: {message}"));
                }
                spotuify_protocol::ReceiptStatus::Confirmed => {
                    self.toast = Some(message);
                }
                spotuify_protocol::ReceiptStatus::Pending => {}
            },
            DaemonEvent::UpdateAvailable {
                latest_version,
                release_url,
                upgrade,
            } => {
                self.update_banner = Some(UpdateBanner::new(
                    latest_version.clone(),
                    release_url,
                    upgrade,
                ));
                self.toast = Some(format!("Update available: {latest_version}"));
            }
            DaemonEvent::MutationFinished { message, .. } => {
                self.toast = Some(message);
            }
            DaemonEvent::EventStreamLagged { skipped } => {
                self.toast = Some(format!("Event stream lagged by {skipped} events"));
            }
            DaemonEvent::AuthError { kind } => {
                self.toast = Some(format!("Authentication needs attention: {kind:?}"));
            }
            DaemonEvent::PlayerDegraded { reason }
            | DaemonEvent::SessionDisconnected { reason }
            | DaemonEvent::PlayerFailed { reason, .. } => {
                self.toast = Some(reason);
            }
            DaemonEvent::PremiumRequired => {
                self.toast = Some("Spotify Premium is required for playback".to_string());
            }
            _ => {}
        }
    }
}

impl Render for DesktopApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        self.ensure_search_input(cx);

        if self
            .playback
            .as_ref()
            .is_some_and(|playback| playback.is_playing)
        {
            // Progress is derived from the daemon's sampled timestamp and the
            // local clock. The app never advances authoritative playback state;
            // this frame request only keeps the rendered seek bar moving.
            window.request_animation_frame();
        }
        match &self.state {
            DesktopState::Booting => diagnostics_surface(
                "connecting...",
                &crate::platform::macos::placeholder_message(),
                vec![
                    "daemon: checking".to_string(),
                    "socket: checking".to_string(),
                    "auth: checking".to_string(),
                    "version: checking".to_string(),
                ],
            )
            .into_any_element(),
            DesktopState::Gate(state) => diagnostics_surface(
                "daemon gate",
                &state.message,
                vec![
                    format!("daemon: {}", health_word(state.daemon_status.running)),
                    format!("socket: {:?}", state.socket_state),
                    format!(
                        "version: {}",
                        state
                            .daemon_status
                            .daemon_version
                            .as_deref()
                            .unwrap_or("unknown")
                    ),
                    format!(
                        "auth: {}",
                        if state.daemon_status.socket_reachable {
                            "available via daemon"
                        } else {
                            "unknown until daemon is reachable"
                        }
                    ),
                ],
            )
            .into_any_element(),
            DesktopState::Connected(state) => self.app_shell(state, cx).into_any_element(),
        }
    }
}

impl DesktopApp {
    fn ensure_search_input(&mut self, cx: &mut Context<'_, Self>) {
        if self.search_input.is_none() {
            let desktop_app = cx.entity().downgrade();
            self.search_input = Some(cx.new(|cx| SearchInput::new(cx, desktop_app)));
        }
    }

    fn app_shell(&self, state: &ConnectedState, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let mut content = div().flex_1().h_full().flex().flex_col().bg(rgb(0x120f14));

        if let Some(banner) = &self.update_banner {
            content = content.child(update_banner_surface(banner));
        }

        content = if self.selected_destination == Destination::Search {
            content.child(self.search_pane(cx))
        } else {
            content.child(self.content_pane(state))
        }
        .child(self.now_playing_footer(cx));

        let mut root = div()
            .size_full()
            .relative()
            .bg(rgb(0x120f14))
            .text_color(rgb(0xf7efe8))
            .flex()
            .flex_row()
            .child(self.sidebar(cx))
            .child(content);

        if let Some(toast) = &self.toast {
            root = root.child(toast_surface(toast));
        }

        root
    }

    fn sidebar(&self, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let mut nav = div()
            .w(px(238.))
            .h_full()
            .bg(rgb(0x211715))
            .border_r_1()
            .border_color(rgb(0x3b2722))
            .px_4()
            .py_5()
            .flex()
            .flex_col();

        nav = nav
            .child(div().text_xs().text_color(rgb(0xa47562)).child("SPOTUIFY"))
            .child(div().mt_1().mb_6().text_2xl().child("Desktop"));

        for destination in Destination::ALL {
            nav = nav.child(nav_item(
                destination,
                destination == self.selected_destination,
                cx,
            ));
        }

        nav
    }

    fn content_pane(&self, state: &ConnectedState) -> impl IntoElement {
        let auth = state
            .doctor_report
            .as_ref()
            .map(|report| report.keychain_token.message.as_str())
            .unwrap_or("unknown");

        div()
            .flex_1()
            .p_8()
            .flex()
            .flex_col()
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .child(div().text_3xl().child(self.selected_destination.label()))
                            .child(
                                div()
                                    .mt_2()
                                    .text_color(rgb(0xb9aca4))
                                    .child(self.selected_destination.stub()),
                            ),
                    )
                    .child(
                        div()
                            .w(px(LAST_EVENT_WIDTH))
                            .flex_shrink_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_xs()
                            .text_color(rgb(0xa9b0bc))
                            .child(format!("last event: {}", state.last_event.as_deref().unwrap_or("none"))),
                    ),
            )
            .child(
                div()
                    .mt_8()
                    .border_1()
                    .border_color(rgb(0x3b2722))
                    .bg(rgb(0x1b1518))
                    .rounded_lg()
                    .p_6()
                    .child(div().text_lg().child(format!("{} pane", self.selected_destination.label())))
                    .child(
                        div()
                            .mt_3()
                            .text_color(rgb(0xb9aca4))
                            .child("This is a routed shell stub. The sidebar already switches panes; data-heavy panes arrive in their own tickets."),
                    )
                    .child(
                        div()
                            .mt_5()
                            .text_sm()
                            .text_color(rgb(0x8f969f))
                            .child(format!(
                                "daemon: {} | auth: {} | version: {}",
                                health_word(state.daemon_status.running),
                                auth,
                                state.daemon_status.daemon_version.as_deref().unwrap_or("unknown")
                            )),
                    ),
            )
    }

    fn search_pane(&self, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let input = self
            .search_input
            .clone()
            .expect("search input should be initialized before rendering");
        let status = if let Some(error) = self.search_error.as_deref() {
            error.to_string()
        } else if self.search_loading {
            "Searching…".to_string()
        } else if self.search_query.trim().is_empty() {
            "Type a query and press Enter".to_string()
        } else if self.search_results.is_empty() {
            "No results".to_string()
        } else {
            format!(
                "{} result{}",
                self.search_results.len(),
                if self.search_results.len() == 1 {
                    ""
                } else {
                    "s"
                }
            )
        };

        let mut results = div()
            .id("search-results")
            .mt_5()
            .flex_1()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_2();
        for (index, item) in self.search_results.iter().enumerate() {
            results = results.child(search_result_row(index, item, cx));
        }

        let mut pane = div()
            .flex_1()
            .p_8()
            .flex()
            .flex_col()
            .child(div().text_3xl().child("Search"))
            .child(
                div()
                    .mt_2()
                    .text_lg()
                    .text_color(rgb(0xd6c7ba))
                    .child("Find tracks, artists, albums, playlists, and episodes."),
            )
            .child(
                div()
                    .mt_6()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().child(input))
                    .child(search_button(cx)),
            )
            .child(
                div()
                    .mt_3()
                    .text_sm()
                    .text_color(if self.search_error.is_some() {
                        rgb(0xf0a0a0)
                    } else {
                        rgb(0x918698)
                    })
                    .child(status),
            )
            .child(results);

        if self.playlist_picker_uri.is_some() {
            pane = pane.child(playlist_picker_surface(self, cx));
        }

        pane
    }

    fn now_playing_footer(&self, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let summary = playback_summary(self.playback.as_ref());
        let playback = self.playback.as_ref();
        let shuffle_state = playback.is_some_and(|playback| playback.shuffle);
        let repeat_state = playback
            .map(|playback| playback.repeat.as_str())
            .unwrap_or("off");
        let is_playing = playback.is_some_and(|playback| playback.is_playing);

        div()
            .h(px(164.))
            .border_t_1()
            .border_color(rgb(0x33252e))
            .bg(rgb(0x19131d))
            .px_6()
            .py_4()
            .flex()
            .items_center()
            .gap_6()
            .child(
                div()
                    .w(px(270.))
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .w(px(66.))
                            .h(px(66.))
                            .rounded_md()
                            .bg(rgb(0x312235))
                            .border_1()
                            .border_color(rgb(0x4a344d)),
                    )
                    .child(
                        div()
                            .ml_4()
                            .child(div().text_lg().child(summary.title))
                            .child(
                                div()
                                    .mt_1()
                                    .text_sm()
                                    .text_color(rgb(0xb8abbf))
                                    .child(summary.subtitle),
                            ),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_3()
                    .child(seek_bar(playback, self.slider_preview, cx))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(transport_button(
                                "previous",
                                "Previous",
                                PlaybackCommand::Previous,
                                cx,
                            ))
                            .child(transport_button(
                                "play-pause",
                                if is_playing { "Pause" } else { "Play" },
                                if is_playing {
                                    PlaybackCommand::Pause
                                } else {
                                    PlaybackCommand::Resume
                                },
                                cx,
                            ))
                            .child(transport_button("next", "Next", PlaybackCommand::Next, cx))
                            .child(transport_button(
                                "shuffle",
                                if shuffle_state {
                                    "Shuffle on"
                                } else {
                                    "Shuffle"
                                },
                                PlaybackCommand::Shuffle {
                                    state: !shuffle_state,
                                },
                                cx,
                            ))
                            .child(transport_button(
                                "repeat",
                                format!("Repeat {repeat_state}"),
                                PlaybackCommand::Repeat {
                                    state: next_repeat_state(repeat_state).to_string(),
                                },
                                cx,
                            )),
                    ),
            )
            .child(
                div()
                    .w(px(160.))
                    .flex()
                    .flex_col()
                    .items_end()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0xf0b78f))
                            .child(summary.state),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0x918698))
                            .child(summary.progress),
                    )
                    .child(volume_bar(playback, self.slider_preview, cx))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0x918698))
                            .child(summary.device),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0x918698))
                            .child(summary.mode),
                    ),
            )
    }
}

fn nav_item(
    destination: Destination,
    selected: bool,
    cx: &mut Context<'_, DesktopApp>,
) -> impl IntoElement {
    let background = if selected { 0x3a211b } else { 0x211715 };
    let foreground = if selected { 0xffc2a6 } else { 0xd8c7bc };

    div()
        .id(SharedString::from(format!("nav-{}", destination.id())))
        .mb_1()
        .px_3()
        .py_2()
        .rounded_md()
        .cursor_pointer()
        .bg(rgb(background))
        .text_color(rgb(foreground))
        .child(destination.label())
        .hover(|style| style.bg(rgb(0x2f211f)))
        .on_click(cx.listener(move |app, _, _, cx| {
            app.selected_destination = destination;
            cx.notify();
        }))
}

fn search_button(cx: &mut Context<'_, DesktopApp>) -> impl IntoElement {
    div()
        .id("search-submit")
        .cursor_pointer()
        .rounded_md()
        .bg(rgb(0x9b604f))
        .px_4()
        .py_3()
        .text_sm()
        .text_color(rgb(0xfff5ee))
        .hover(|style| style.bg(rgb(0xb8745b)))
        .on_click(cx.listener(|app, _, _, cx| {
            app.start_search();
            cx.notify();
        }))
        .child("Search")
}

fn search_result_row(
    index: usize,
    item: &MediaItem,
    cx: &mut Context<'_, DesktopApp>,
) -> impl IntoElement {
    let uri = item.uri.clone();
    let title = if item.name.is_empty() {
        "Untitled result".to_string()
    } else {
        item.name.clone()
    };
    let subtitle = if item.subtitle.is_empty() {
        item.kind.to_string()
    } else {
        format!("{} · {}", item.subtitle, item.kind)
    };
    let play_uri = uri.clone();
    let play_title = title.clone();
    let queue_uri = uri.clone();
    let queue_title = title.clone();
    let add_uri = uri.clone();
    let can_queue = matches!(
        item.kind,
        MediaKind::Track | MediaKind::Episode | MediaKind::Album | MediaKind::Playlist
    );
    let can_add_to_playlist = matches!(item.kind, MediaKind::Track | MediaKind::Episode);

    let mut row = div()
        .id(SharedString::from(format!("search-result-{index}")))
        .w_full()
        .rounded_md()
        .border_1()
        .border_color(rgb(0x3b2722))
        .bg(rgb(0x1b1518))
        .px_4()
        .py_3()
        .flex()
        .items_center()
        .gap_3()
        .child(
            div()
                .flex_1()
                .overflow_hidden()
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(0xf7efe8))
                        .truncate()
                        .child(title),
                )
                .child(
                    div()
                        .mt_1()
                        .text_xs()
                        .text_color(rgb(0x9e929d))
                        .truncate()
                        .child(subtitle),
                ),
        )
        .child(search_row_action(
            "Play",
            cx.listener(move |app, _, _, cx| {
                app.send_playback_command(PlaybackCommand::PlayUri {
                    uri: play_uri.clone(),
                    context_uri: None,
                });
                app.toast = Some(format!("Playing {play_title}"));
                cx.notify();
            }),
        ));

    if can_queue {
        row = row.child(search_row_action(
            "Queue",
            cx.listener(move |app, _, _, cx| {
                app.send_request(Request::QueueAdd {
                    uri: queue_uri.clone(),
                });
                app.toast = Some(format!("Queued {queue_title}"));
                cx.notify();
            }),
        ));
    }

    if can_add_to_playlist {
        row = row.child(search_row_action(
            "Add to playlist",
            cx.listener(move |app, _, _, cx| {
                app.open_playlist_picker(add_uri.clone());
                cx.notify();
            }),
        ));
    }

    row
}

fn search_row_action(
    label: &'static str,
    listener: impl Fn(&MouseUpEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .cursor_pointer()
        .rounded_md()
        .border_1()
        .border_color(rgb(0x4a344d))
        .px_3()
        .py_2()
        .text_xs()
        .text_color(rgb(0xf1e8ff))
        .hover(|style| style.bg(rgb(0x493052)))
        .on_mouse_up(MouseButton::Left, listener)
        .child(label)
}

fn playlist_picker_surface(app: &DesktopApp, cx: &mut Context<'_, DesktopApp>) -> impl IntoElement {
    let mut picker = div()
        .mt_5()
        .rounded_lg()
        .border_1()
        .border_color(rgb(0x6f3a22))
        .bg(rgb(0x24171a))
        .p_4()
        .child(div().text_sm().child("Choose a playlist"));

    if app.playlist_loading {
        picker = picker.child(
            div()
                .mt_2()
                .text_xs()
                .text_color(rgb(0x9e929d))
                .child("Loading playlists…"),
        );
    } else if app.search_playlists.is_empty() {
        picker = picker.child(
            div()
                .mt_2()
                .text_xs()
                .text_color(rgb(0x9e929d))
                .child("No playlists available"),
        );
    } else {
        let mut playlists = div().mt_3().flex().flex_wrap().gap_2();
        for playlist in &app.search_playlists {
            let playlist_id = playlist.id.clone();
            let playlist_name = playlist.name.clone();
            playlists = playlists.child(
                div()
                    .id(SharedString::from(format!(
                        "playlist-picker-{}",
                        playlist.id
                    )))
                    .cursor_pointer()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(0x4a344d))
                    .px_3()
                    .py_2()
                    .text_xs()
                    .text_color(rgb(0xf1e8ff))
                    .hover(|style| style.bg(rgb(0x493052)))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(move |app, _, _, cx| {
                            app.add_search_result_to_playlist(playlist_id.clone());
                            cx.notify();
                        }),
                    )
                    .child(playlist_name),
            );
        }
        picker = picker.child(playlists);
    }

    picker.child(
        div()
            .mt_3()
            .cursor_pointer()
            .text_xs()
            .text_color(rgb(0xd9ac92))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|app, _, _, cx| {
                    app.playlist_picker_uri = None;
                    app.playlist_loading = false;
                    cx.notify();
                }),
            )
            .child("Cancel"),
    )
}

struct SearchInput {
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    desktop_app: WeakEntity<DesktopApp>,
    selected_range: Range<usize>,
    marked_range: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
}

impl SearchInput {
    fn new(cx: &mut Context<Self>, desktop_app: WeakEntity<DesktopApp>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            content: SharedString::default(),
            placeholder: "Search Spotify".into(),
            desktop_app,
            selected_range: 0..0,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
        }
    }

    fn notify_owner(&self, cx: &mut Context<Self>) {
        let query = self.content.to_string();
        let _ = self.desktop_app.update(cx, |app, cx| {
            app.update_search_query(query);
            cx.notify();
        });
    }

    fn submit(&self, cx: &mut Context<Self>) {
        let _ = self.desktop_app.update(cx, |app, cx| {
            app.start_search();
            cx.notify();
        });
    }

    fn cursor_offset(&self) -> usize {
        self.selected_range.end
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        cx.notify();
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content[..offset]
            .char_indices()
            .last()
            .map(|(index, _)| index)
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content[offset..]
            .char_indices()
            .nth(1)
            .map(|(index, _)| offset + index)
            .unwrap_or(self.content.len())
    }

    fn replace_text(&mut self, range: Range<usize>, text: &str, cx: &mut Context<Self>) {
        self.content = format!(
            "{}{}{}",
            &self.content[..range.start],
            text,
            &self.content[range.end..]
        )
        .into();
        let cursor = range.start + text.len();
        self.selected_range = cursor..cursor;
        self.marked_range = None;
        self.notify_owner(cx);
        cx.notify();
    }

    fn backspace(&mut self, cx: &mut Context<Self>) {
        let range = if self.selected_range.is_empty() {
            let cursor = self.cursor_offset();
            self.previous_boundary(cursor)..cursor
        } else {
            self.selected_range.clone()
        };
        if !range.is_empty() {
            self.replace_text(range, "", cx);
        }
    }

    fn delete(&mut self, cx: &mut Context<Self>) {
        let range = if self.selected_range.is_empty() {
            let cursor = self.cursor_offset();
            cursor..self.next_boundary(cursor)
        } else {
            self.selected_range.clone()
        };
        if !range.is_empty() {
            self.replace_text(range, "", cx);
        }
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        let (Some(bounds), Some(line)) = (&self.last_bounds, &self.last_layout) else {
            return self.content.len();
        };
        line.index_for_x(position.x - bounds.left())
            .unwrap_or(self.content.len())
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            "enter" => self.submit(cx),
            "backspace" => self.backspace(cx),
            "delete" => self.delete(cx),
            "left" => self.move_to(self.previous_boundary(self.cursor_offset()), cx),
            "right" => self.move_to(self.next_boundary(self.cursor_offset()), cx),
            "home" => self.move_to(0, cx),
            "end" => self.move_to(self.content.len(), cx),
            _ => {}
        }
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_handle.focus(window);
        self.move_to(self.index_for_mouse_position(event.position), cx);
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {}

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        for (index, ch) in self.content.char_indices() {
            if utf16_offset >= offset {
                return index;
            }
            utf16_offset += ch.len_utf16();
        }
        self.content.len()
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        self.content[..offset].chars().map(char::len_utf16).sum()
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }
}

impl EntityInputHandler for SearchInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: false,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or(self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range.clone());
        self.replace_text(range, new_text, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or(self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range.clone());
        self.content = format!(
            "{}{}{}",
            &self.content[..range.start],
            new_text,
            &self.content[range.end..]
        )
        .into();
        self.marked_range =
            (!new_text.is_empty()).then_some(range.start..range.start + new_text.len());
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|selected| self.range_from_utf16(selected))
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());
        self.notify_owner(cx);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let line = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        Some(Bounds::from_corners(
            point(bounds.left() + line.x_for_index(range.start), bounds.top()),
            point(bounds.left() + line.x_for_index(range.end), bounds.bottom()),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.index_for_mouse_position(point))
    }
}

impl Focusable for SearchInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

struct SearchTextElement {
    input: Entity<SearchInput>,
}

struct SearchTextPrepaint {
    line: ShapedLine,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
}

impl IntoElement for SearchTextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for SearchTextElement {
    type RequestLayoutState = ();
    type PrepaintState = SearchTextPrepaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let display_text = if input.content.is_empty() {
            input.placeholder.clone()
        } else {
            input.content.clone()
        };
        let text_color = if input.content.is_empty() {
            window.text_style().color.opacity(0.55)
        } else {
            window.text_style().color
        };
        let run = TextRun {
            len: display_text.len(),
            font: window.text_style().font(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let font_size = window.text_style().font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display_text, font_size, &[run], None);
        let cursor_pos = line.x_for_index(input.cursor_offset());
        let cursor = if input.focus_handle.is_focused(window) {
            Some(fill(
                Bounds::new(
                    point(bounds.left() + cursor_pos, bounds.top()),
                    gpui::size(px(2.), bounds.bottom() - bounds.top()),
                ),
                rgb(0xf0b78f),
            ))
        } else {
            None
        };
        let selection = (!input.selected_range.is_empty()).then(|| {
            fill(
                Bounds::from_corners(
                    point(
                        bounds.left() + line.x_for_index(input.selected_range.start),
                        bounds.top(),
                    ),
                    point(
                        bounds.left() + line.x_for_index(input.selected_range.end),
                        bounds.bottom(),
                    ),
                ),
                rgb(0x493052),
            )
        });
        SearchTextPrepaint {
            line,
            cursor,
            selection,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection);
        }
        prepaint
            .line
            .paint(bounds.origin, window.line_height(), window, cx)
            .expect("search input text should paint");
        if let Some(cursor) = prepaint.cursor.take() {
            window.paint_quad(cursor);
        }
        self.input.update(cx, |input, _| {
            input.last_layout = Some(prepaint.line.clone());
            input.last_bounds = Some(bounds);
        });
    }
}

impl Render for SearchInput {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("search-input")
            .h(px(42.))
            .flex()
            .key_context("SearchInput")
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .on_key_down(cx.listener(Self::on_key_down))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .rounded_md()
            .border_1()
            .border_color(rgb(0x4a344d))
            .bg(rgb(0x211715))
            .px_3()
            .items_center()
            .text_sm()
            .child(SearchTextElement { input: cx.entity() })
    }
}

fn update_banner_surface(banner: &UpdateBanner) -> impl IntoElement {
    let detail = banner
        .command
        .as_deref()
        .or(banner.release_url.as_deref())
        .unwrap_or("Open the latest release to upgrade.");

    div()
        .mx_6()
        .mt_5()
        .rounded_lg()
        .border_1()
        .border_color(rgb(0x6f3a22))
        .bg(rgb(0x321d16))
        .px_4()
        .py_3()
        .child(format!("Update available: {}", banner.latest_version))
        .child(
            div()
                .mt_1()
                .text_sm()
                .text_color(rgb(0xd9ac92))
                .child(detail.to_string()),
        )
}

fn toast_surface(message: &str) -> impl IntoElement {
    div()
        .absolute()
        .right_6()
        .top_6()
        .max_w(px(420.))
        .rounded_lg()
        .border_1()
        .border_color(rgb(0x4c3946))
        .bg(rgb(0x201926))
        .px_4()
        .py_3()
        .text_sm()
        .text_color(rgb(0xf1e8ff))
        .child(message.to_string())
}

const SLIDER_WIDTH: f32 = 420.0;
const LAST_EVENT_WIDTH: f32 = 360.0;
const MAX_EVENT_LABEL_CHARS: usize = 48;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SliderKind {
    Seek,
    Volume,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SliderPreview {
    Seek(u64),
    Volume(u8),
}

#[derive(Clone, Copy, Debug)]
struct SeekDrag;

#[derive(Clone, Copy, Debug)]
struct VolumeDrag;

struct SliderGhost;

impl Render for SliderGhost {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<'_, Self>) -> impl IntoElement {
        div().size_0()
    }
}

fn transport_button(
    id: &'static str,
    label: impl Into<SharedString>,
    command: PlaybackCommand,
    cx: &mut Context<'_, DesktopApp>,
) -> impl IntoElement {
    div()
        .id(SharedString::from(format!("transport-{id}")))
        .cursor_pointer()
        .rounded_md()
        .border_1()
        .border_color(rgb(0x4a344d))
        .bg(rgb(0x2a1b2f))
        .px_3()
        .py_2()
        .text_xs()
        .text_color(rgb(0xf1e8ff))
        .hover(|style| style.bg(rgb(0x493052)))
        .on_click(cx.listener(move |app, _, _, _| {
            app.send_playback_command(command.clone());
        }))
        .child(label.into())
}

fn seek_bar(
    playback: Option<&Playback>,
    preview: Option<SliderPreview>,
    cx: &mut Context<'_, DesktopApp>,
) -> impl IntoElement {
    let fraction = playback
        .and_then(|playback| playback.item.as_ref().map(|item| (playback, item)))
        .filter(|(_, item)| item.duration_ms > 0)
        .map_or(0.0, |(playback, item)| {
            let position_ms = match preview {
                Some(SliderPreview::Seek(position_ms)) => position_ms,
                _ => playback_progress_ms(playback),
            };
            (position_ms as f32 / item.duration_ms as f32).clamp(0.0, 1.0)
        });

    div()
        .id("seek-bar")
        .w(px(SLIDER_WIDTH))
        .h(px(14.))
        .cursor_pointer()
        .rounded_md()
        .bg(rgb(0x34283b))
        .child(
            div()
                .h_full()
                .w(px(SLIDER_WIDTH * fraction))
                .rounded_md()
                .bg(rgb(0xf0b78f)),
        )
        .on_drag(SeekDrag, |_, _, _, cx| cx.new(|_| SliderGhost))
        .on_drag_move(cx.listener(|app, event: &DragMoveEvent<SeekDrag>, _, cx| {
            if let Some(item) = app
                .playback
                .as_ref()
                .and_then(|playback| playback.item.as_ref())
            {
                let position_ms = seek_position_ms(item.duration_ms, slider_fraction(event));
                app.preview_slider(SliderKind::Seek, SliderPreview::Seek(position_ms));
                cx.notify();
            }
        }))
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|app, _, _, cx| {
                app.finish_slider_drag(SliderKind::Seek);
                cx.notify();
            }),
        )
        .on_mouse_up_out(
            MouseButton::Left,
            cx.listener(|app, _, _, cx| {
                app.finish_slider_drag(SliderKind::Seek);
                cx.notify();
            }),
        )
}

fn volume_bar(
    playback: Option<&Playback>,
    preview: Option<SliderPreview>,
    cx: &mut Context<'_, DesktopApp>,
) -> impl IntoElement {
    let device = playback.and_then(|playback| playback.device.as_ref());
    let fraction = device
        .and_then(|device| device.volume_percent)
        .map_or(0.0, |volume| f32::from(volume) / 100.0);
    let fraction = match preview {
        Some(SliderPreview::Volume(volume_percent)) => f32::from(volume_percent) / 100.0,
        _ => fraction,
    };
    let supports_volume = device.is_some_and(|device| device.supports_volume);

    let bar = div()
        .id("volume-bar")
        .w(px(140.))
        .h(px(10.))
        .cursor_pointer()
        .rounded_md()
        .bg(rgb(0x34283b))
        .child(
            div()
                .h_full()
                .w(px(140. * fraction))
                .rounded_md()
                .bg(rgb(0x9fc7c5)),
        );

    if supports_volume {
        bar.on_drag(VolumeDrag, |_, _, _, cx| cx.new(|_| SliderGhost))
            .on_drag_move(
                cx.listener(|app, event: &DragMoveEvent<VolumeDrag>, _, cx| {
                    let volume_percent = volume_percent(slider_fraction(event));
                    app.preview_slider(SliderKind::Volume, SliderPreview::Volume(volume_percent));
                    cx.notify();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|app, _, _, cx| {
                    app.finish_slider_drag(SliderKind::Volume);
                    cx.notify();
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|app, _, _, cx| {
                    app.finish_slider_drag(SliderKind::Volume);
                    cx.notify();
                }),
            )
    } else {
        bar.opacity(0.45)
    }
}

fn slider_fraction<T>(event: &DragMoveEvent<T>) -> f32 {
    ((event.event.position.x - event.bounds.origin.x) / event.bounds.size.width).clamp(0.0, 1.0)
}

fn next_repeat_state(repeat: &str) -> &'static str {
    match repeat {
        "off" => "context",
        "context" => "track",
        "track" => "off",
        _ => "off",
    }
}

fn seek_position_ms(duration_ms: u64, fraction: f32) -> u64 {
    (duration_ms as f32 * fraction.clamp(0.0, 1.0)).round() as u64
}

fn volume_percent(fraction: f32) -> u8 {
    (fraction.clamp(0.0, 1.0) * 100.0).round() as u8
}

struct PlaybackSummary {
    title: String,
    subtitle: String,
    state: String,
    progress: String,
    device: String,
    mode: String,
}

fn playback_summary(playback: Option<&Playback>) -> PlaybackSummary {
    let Some(playback) = playback else {
        return PlaybackSummary {
            title: "Nothing playing".to_string(),
            subtitle: "Waiting for daemon playback state".to_string(),
            state: "Idle".to_string(),
            progress: "0:00".to_string(),
            device: "No active device".to_string(),
            mode: "shuffle off | repeat off".to_string(),
        };
    };

    let item = playback.item.as_ref();
    let title = item
        .map(|item| item.name.clone())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Unknown track".to_string());
    let subtitle = item
        .map(media_subtitle)
        .unwrap_or_else(|| "No media item".to_string());
    let duration_ms = item.map_or(0, |item| item.duration_ms);
    let state = if playback.is_playing {
        "Playing"
    } else {
        "Paused"
    }
    .to_string();
    let progress_ms = playback_progress_ms(playback);
    let progress = if duration_ms == 0 {
        format_duration(progress_ms)
    } else {
        format!(
            "{} / {}",
            format_duration(progress_ms),
            format_duration(duration_ms)
        )
    };
    let device = playback
        .device
        .as_ref()
        .map(|device| format!("{} | {}", device.name, device.kind))
        .unwrap_or_else(|| "No active device".to_string());
    let mode = format!(
        "shuffle {} | repeat {}",
        if playback.shuffle { "on" } else { "off" },
        playback.repeat
    );

    PlaybackSummary {
        title,
        subtitle,
        state,
        progress,
        device,
        mode,
    }
}

fn playback_progress_ms(playback: &Playback) -> u64 {
    if !playback.is_playing {
        return playback.progress_ms;
    }

    let Some(sampled_at_ms) = playback.sampled_at_ms else {
        return playback.progress_ms;
    };
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(sampled_at_ms, |duration| duration.as_millis() as i64);
    let elapsed_ms = now_ms.saturating_sub(sampled_at_ms) as u64;
    let projected = playback.progress_ms.saturating_add(elapsed_ms);
    playback
        .item
        .as_ref()
        .map_or(projected, |item| projected.min(item.duration_ms))
}

fn media_subtitle(item: &MediaItem) -> String {
    if !item.subtitle.is_empty() {
        item.subtitle.clone()
    } else if !item.context.is_empty() {
        item.context.clone()
    } else {
        item.kind.to_string()
    }
}

fn format_duration(ms: u64) -> String {
    let total_seconds = ms / 1_000;
    format!("{}:{:02}", total_seconds / 60, total_seconds % 60)
}

fn event_label(event: &DaemonEvent) -> String {
    let label = match event {
        DaemonEvent::ShutdownRequested => "shutdown-requested".to_string(),
        DaemonEvent::PlaybackChanged { action, .. } => format!("playback:{action}"),
        DaemonEvent::QueueChanged { action, .. } => format!("queue:{action}"),
        DaemonEvent::DevicesChanged { action, .. } => format!("devices:{action}"),
        DaemonEvent::PlaylistsChanged { action, .. } => format!("playlists:{action}"),
        DaemonEvent::LibraryChanged { action, .. } => format!("library:{action}"),
        DaemonEvent::SearchUpdated { count, .. } => format!("search-updated:{count}"),
        DaemonEvent::SearchPage { kind, offset, .. } => {
            format!("search-page:{kind:?}:{offset}")
        }
        DaemonEvent::SearchComplete { .. } => "search-complete".to_string(),
        DaemonEvent::SearchFailed { .. } => "search-failed".to_string(),
        DaemonEvent::EventStreamLagged { skipped } => format!("event-stream-lagged:{skipped}"),
        DaemonEvent::SyncStarted { target } => format!("sync-started:{}", target.label()),
        DaemonEvent::SyncFinished { summary } => {
            format!("sync-finished:{}", summary.target.label())
        }
        DaemonEvent::MutationFinished { action, .. } => format!("mutation:{action}"),
        DaemonEvent::RateLimited { scope, .. } => format!("rate-limited:{scope}"),
        DaemonEvent::AuthError { kind } => format!("auth-error:{kind:?}"),
        DaemonEvent::MutationAccepted { action, .. } => format!("mutation-accepted:{action}"),
        DaemonEvent::MutationFinalized { .. } => "mutation-finalized".to_string(),
        DaemonEvent::SchemaCompat { endpoint, .. } => format!("schema-compat:{endpoint}"),
        DaemonEvent::PlayerReady { name, .. } => format!("player-ready:{name}"),
        DaemonEvent::PlayerDegraded { .. } => "player-degraded".to_string(),
        DaemonEvent::PremiumRequired => "premium-required".to_string(),
        DaemonEvent::SessionDisconnected { .. } => "session-disconnected".to_string(),
        DaemonEvent::PlayerFailed { .. } => "player-failed".to_string(),
        DaemonEvent::ListenQualified { .. } => "listen-qualified".to_string(),
        DaemonEvent::AnalyticsImportProgress { phase, .. } => {
            format!("analytics-import:{phase}")
        }
        DaemonEvent::OperationRecorded { .. } => "operation-recorded".to_string(),
        DaemonEvent::OperationUndone { success, .. } => {
            format!(
                "operation-undone:{}",
                if *success { "ok" } else { "failed" }
            )
        }
        DaemonEvent::ConfigReloaded => "config-reloaded".to_string(),
        DaemonEvent::SpectrumFrame { .. } => "spectrum-frame".to_string(),
        DaemonEvent::VizSourceChanged { .. } => "viz-source-changed".to_string(),
        DaemonEvent::ReminderDue { .. } => "reminder-due".to_string(),
        DaemonEvent::RemindersChanged { action } => format!("reminders:{action}"),
        DaemonEvent::UpdateAvailable { latest_version, .. } => {
            format!("update-available:{latest_version}")
        }
        DaemonEvent::Unknown => "unknown".to_string(),
    };

    bounded_event_label(label)
}

fn bounded_event_label(label: String) -> String {
    if label.chars().count() <= MAX_EVENT_LABEL_CHARS {
        return label;
    }

    let mut bounded = label
        .chars()
        .take(MAX_EVENT_LABEL_CHARS.saturating_sub(1))
        .collect::<String>();
    bounded.push('…');
    bounded
}

fn should_toast_playback_action(action: &str) -> bool {
    !matches!(action, "snapshot" | "synced" | "sync" | "poll") && !action.starts_with("optimistic-")
}

fn diagnostics_surface(title: &str, body: &str, lines: Vec<String>) -> impl IntoElement {
    let mut status_rows = div().mt_6().flex().flex_col();
    for line in lines {
        status_rows =
            status_rows.child(div().mb_2().text_sm().text_color(rgb(0xa9b0bc)).child(line));
    }

    div()
        .size_full()
        .bg(rgb(0x1d1413))
        .p_8()
        .flex()
        .flex_col()
        .items_start()
        .justify_start()
        .text_color(rgb(0xf7efe8))
        .child(div().text_3xl().child(title.to_string()))
        .child(
            div()
                .mt_2()
                .text_lg()
                .text_color(rgb(0xd6c7ba))
                .child(body.to_string()),
        )
        .child(status_rows)
}

fn health_word(is_healthy: bool) -> &'static str {
    if is_healthy {
        "healthy"
    } else {
        "down"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spotuify_core::{MediaKind, Playback};
    use spotuify_protocol::{UpgradeMethod, IPC_PROTOCOL_VERSION};

    #[test]
    fn sidebar_selection_updates_destination() {
        let mut app = DesktopApp::new();

        app.selected_destination = Destination::Queue;

        assert_eq!(app.selected_destination, Destination::Queue);
    }

    #[test]
    fn playback_event_updates_footer_state() {
        let mut app = connected_app();
        let playback = Playback {
            item: Some(MediaItem {
                name: "C.R.E.A.M.".to_string(),
                subtitle: "Wu-Tang Clan".to_string(),
                duration_ms: 238_000,
                kind: MediaKind::Track,
                ..MediaItem::default()
            }),
            is_playing: true,
            progress_ms: 61_000,
            repeat: "context".to_string(),
            ..Playback::default()
        };

        app.apply_daemon_event(DaemonEvent::PlaybackChanged {
            action: "optimistic-next".to_string(),
            playback: Some(playback),
        });

        let summary = playback_summary(app.playback.as_ref());
        assert_eq!(summary.title, "C.R.E.A.M.");
        assert_eq!(summary.progress, "1:01 / 3:58");
        assert_eq!(app.toast.as_deref(), None);
    }

    #[test]
    fn transport_state_round_trips_from_playback_event() {
        let mut app = connected_app();
        app.apply_daemon_event(DaemonEvent::PlaybackChanged {
            action: "optimistic-shuffle".to_string(),
            playback: Some(Playback {
                shuffle: true,
                repeat: "track".to_string(),
                ..Playback::default()
            }),
        });

        let playback = app.playback.expect("playback event should seed state");
        assert!(playback.shuffle);
        assert_eq!(playback.repeat, "track");
    }

    #[test]
    fn transport_controls_enqueue_protocol_mutations() {
        let mut app = DesktopApp::new();
        let (command_tx, mut command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (slider_tx, slider_rx) = watch::channel::<Option<Request>>(None);
        app.set_command_senders(command_tx, slider_tx);

        app.send_playback_command(PlaybackCommand::Pause);
        assert!(matches!(
            command_rx.try_recv(),
            Ok(Request::PlaybackCommand {
                command: PlaybackCommand::Pause
            })
        ));

        app.send_slider_command(PlaybackCommand::Seek {
            position_ms: 12_345,
        });
        app.send_slider_command(PlaybackCommand::Seek {
            position_ms: 98_765,
        });
        assert_eq!(
            slider_rx.borrow().clone(),
            Some(Request::PlaybackCommand {
                command: PlaybackCommand::Seek {
                    position_ms: 98_765,
                },
            })
        );
    }

    #[test]
    fn starting_search_enqueues_versioned_stream_request() {
        let mut app = DesktopApp::new();
        let (command_tx, mut command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (slider_tx, _) = watch::channel::<Option<Request>>(None);
        app.set_command_senders(command_tx, slider_tx);
        app.search_query = "  radiohead  ".to_string();

        app.start_search();

        assert!(app.search_loading);
        assert_eq!(app.search_version, 1);
        assert!(matches!(
            command_rx.try_recv(),
            Ok(Request::SearchStream {
                query,
                scope: SearchScopeData::All,
                source: SearchSourceData::Spotify,
                version: 1,
            }) if query == "radiohead"
        ));
    }

    #[test]
    fn search_events_ignore_stale_versions_and_complete_current_query() {
        let mut app = connected_app();
        app.search_query = "radiohead".to_string();
        app.search_version = 2;
        let item = MediaItem {
            name: "Everything In Its Right Place".to_string(),
            uri: "spotify:track:1".to_string(),
            kind: MediaKind::Track,
            ..MediaItem::default()
        };

        app.apply_daemon_event(DaemonEvent::SearchPage {
            query: "radiohead".to_string(),
            kind: MediaKind::Track,
            offset: 0,
            version: 1,
            items: vec![item.clone()],
        });
        assert!(app.search_results.is_empty());

        app.search_loading = true;
        app.apply_daemon_event(DaemonEvent::SearchPage {
            query: "radiohead".to_string(),
            kind: MediaKind::Track,
            offset: 0,
            version: 2,
            items: vec![item],
        });
        assert_eq!(app.search_results.len(), 1);

        app.apply_daemon_event(DaemonEvent::SearchComplete {
            query: "radiohead".to_string(),
            version: 2,
        });
        assert!(!app.search_loading);
    }

    #[test]
    fn playlist_response_completes_picker_loading_state() {
        let mut app = DesktopApp::new();
        app.playlist_picker_uri = Some("spotify:track:1".to_string());
        app.playlist_loading = true;

        app.apply_daemon_response(ResponseData::Playlists {
            playlists: vec![Playlist {
                id: "playlist-1".to_string(),
                name: "Favorites".to_string(),
                owner: "me".to_string(),
                tracks_total: 1,
                image_url: None,
                snapshot_id: None,
            }],
        });

        assert!(!app.playlist_loading);
        assert_eq!(app.search_playlists[0].name, "Favorites");
    }

    #[test]
    fn selecting_playlist_enqueues_add_request_and_closes_picker() {
        let mut app = DesktopApp::new();
        let (command_tx, mut command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (slider_tx, _) = watch::channel::<Option<Request>>(None);
        app.set_command_senders(command_tx, slider_tx);
        app.playlist_picker_uri = Some("spotify:track:1".to_string());

        app.add_search_result_to_playlist("playlist-1".to_string());

        assert_eq!(app.playlist_picker_uri, None);
        assert!(matches!(
            command_rx.try_recv(),
            Ok(Request::PlaylistAddItems { playlist, uris })
                if playlist == "playlist-1" && uris == vec!["spotify:track:1"]
        ));
    }

    #[test]
    fn slider_preview_is_local_until_release_then_commits_latest_value() {
        let mut app = DesktopApp::new();
        let (slider_tx, slider_rx) = watch::channel::<Option<Request>>(None);
        app.slider_tx = Some(slider_tx);

        app.preview_slider(SliderKind::Seek, SliderPreview::Seek(98_765));
        assert_eq!(
            app.slider_preview,
            Some(SliderPreview::Seek(98_765)),
            "drag feedback should be available before the daemon replies"
        );
        assert_eq!(*slider_rx.borrow(), None);

        app.finish_slider_drag(SliderKind::Seek);

        assert_eq!(app.slider_preview, None);
        assert_eq!(
            *slider_rx.borrow(),
            Some(Request::PlaybackCommand {
                command: PlaybackCommand::Seek {
                    position_ms: 98_765,
                },
            })
        );
    }

    #[test]
    fn releasing_over_another_slider_does_not_abort_the_active_preview() {
        let mut app = DesktopApp::new();
        let (slider_tx, slider_rx) = watch::channel::<Option<Request>>(None);
        app.slider_tx = Some(slider_tx);

        app.preview_slider(SliderKind::Seek, SliderPreview::Seek(12_345));
        app.finish_slider_drag(SliderKind::Volume);

        assert_eq!(app.slider_preview, Some(SliderPreview::Seek(12_345)));
        assert_eq!(*slider_rx.borrow(), None);
    }

    #[test]
    fn transport_value_mapping_clamps_and_rounds() {
        assert_eq!(seek_position_ms(200_000, 0.25), 50_000);
        assert_eq!(seek_position_ms(200_000, -1.0), 0);
        assert_eq!(seek_position_ms(200_000, 2.0), 200_000);
        assert_eq!(volume_percent(0.505), 51);
        assert_eq!(volume_percent(-1.0), 0);
        assert_eq!(volume_percent(2.0), 100);
    }

    #[test]
    fn playing_progress_is_derived_from_daemon_sample_time() {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after the Unix epoch")
            .as_millis() as i64;
        let playback = Playback {
            item: Some(MediaItem {
                duration_ms: 10_000,
                ..MediaItem::default()
            }),
            is_playing: true,
            progress_ms: 1_000,
            sampled_at_ms: Some(now_ms - 2_000),
            ..Playback::default()
        };

        assert!(playback_progress_ms(&playback) >= 2_900);
        assert!(playback_progress_ms(&playback) <= 10_000);
    }

    #[test]
    fn repeat_button_cycles_through_daemon_modes() {
        assert_eq!(next_repeat_state("off"), "context");
        assert_eq!(next_repeat_state("context"), "track");
        assert_eq!(next_repeat_state("track"), "off");
        assert_eq!(next_repeat_state("unknown"), "off");
    }

    #[test]
    fn update_event_sets_banner() {
        let mut app = connected_app();

        app.apply_daemon_event(DaemonEvent::UpdateAvailable {
            latest_version: "0.1.79".to_string(),
            release_url: None,
            upgrade: UpgradeHint {
                method: UpgradeMethod::Homebrew,
                command: Some("brew upgrade spotuify".to_string()),
                url: Some("https://example.test/release".to_string()),
            },
        });

        assert_eq!(
            app.update_banner
                .as_ref()
                .map(|banner| banner.latest_version.as_str()),
            Some("0.1.79")
        );
        assert_eq!(
            app.update_banner
                .as_ref()
                .and_then(|banner| banner.command.as_deref()),
            Some("brew upgrade spotuify")
        );
        assert_eq!(app.toast.as_deref(), Some("Update available: 0.1.79"));
    }

    #[test]
    fn diagnostic_event_label_does_not_render_full_sync_payload() {
        let label = event_label(&DaemonEvent::SyncFinished {
            summary: spotuify_protocol::CacheSyncSummary {
                target: spotuify_protocol::SyncTargetData::Queue,
                playback_snapshots: 0,
                queue_snapshots: 0,
                queue_items: 20,
                devices: 0,
                playlists: 0,
                playlist_items: 0,
                recent_items: 0,
                library_items: 0,
                media_items: 0,
            },
        });

        assert_eq!(label, "sync-finished:queue");
        assert!(label.chars().count() <= MAX_EVENT_LABEL_CHARS);
    }

    fn connected_app() -> DesktopApp {
        let mut app = DesktopApp::new();
        app.state = DesktopState::Connected(ConnectedState {
            daemon_status: DaemonStatus {
                running: true,
                socket_path: "test.sock".to_string(),
                socket_exists: true,
                socket_reachable: true,
                stale_socket: false,
                daemon_pid: None,
                uptime_secs: None,
                protocol_version: IPC_PROTOCOL_VERSION,
                daemon_version: Some("test".to_string()),
                daemon_build_id: None,
                audio_health: None,
            },
            doctor_report: None,
            last_event: None,
        });
        app
    }
}
