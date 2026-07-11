use gpui::prelude::*;
use gpui::{div, rgb, Window};
use spotuify_launcher::SocketState;
use spotuify_protocol::{DaemonStatus, DoctorReport};

pub struct DesktopApp {
    pub(crate) state: DesktopState,
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
    pub(crate) doctor_report: Option<DoctorReport>,
    pub(crate) last_event: Option<String>,
}

impl DesktopApp {
    pub fn new() -> Self {
        Self {
            state: DesktopState::Booting,
        }
    }
}

impl Render for DesktopApp {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<'_, Self>) -> impl IntoElement {
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
            ),
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
            ),
            DesktopState::Connected(state) => diagnostics_surface(
                "daemon connected",
                "Subscribed to daemon events.",
                vec![
                    format!("daemon: {}", health_word(state.daemon_status.running)),
                    "socket: reachable".to_string(),
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
                        state
                            .doctor_report
                            .as_ref()
                            .map(|report| report.keychain_token.message.as_str())
                            .unwrap_or("unknown")
                    ),
                    format!(
                        "last event: {}",
                        state.last_event.as_deref().unwrap_or("none")
                    ),
                ],
            ),
        }
    }
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
