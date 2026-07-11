pub mod macos;

use gpui::AppContext;
use gpui::{Application, WindowOptions};
use spotuify_launcher::{daemon_status, ensure_daemon_running, inspect_socket_state, SocketState};
use spotuify_protocol::{DaemonEvent, DaemonStatus, Request, Response, ResponseData};

use crate::views::DesktopApp;

pub fn run() {
    Application::new().run(move |app| {
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

    let doctor_report = match client.request(Request::GetDoctorReport).await {
        Ok(Response::Ok {
            data: ResponseData::DoctorReport { report },
        }) => Some(report),
        _ => None,
    };

    let status = daemon_status()
        .await
        .unwrap_or_else(|_| fallback_status(&socket_path, socket_state, true));
    let _ = view.update(&mut cx, |app, cx| {
        app.state = crate::views::DesktopState::Connected(crate::views::ConnectedState {
            daemon_status: status,
            doctor_report,
            last_event: None,
        });
        cx.notify();
    });

    loop {
        let Ok(event) = client.next_event().await else {
            break;
        };

        let event_label = match event {
            DaemonEvent::AuthError { kind } => format!("auth-error:{kind:?}"),
            DaemonEvent::PlaybackChanged { action, .. } => format!("playback:{action}"),
            other => format!("{other:?}"),
        };

        let _ = view.update(&mut cx, |app, cx| {
            if let crate::views::DesktopState::Connected(state) = &mut app.state {
                state.last_event = Some(event_label.clone());
            }
            cx.notify();
        });
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
