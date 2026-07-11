use gpui::prelude::*;
use gpui::{div, px, rgb, Context, IntoElement, SharedString, Window};
use spotuify_core::{MediaItem, Playback};
use spotuify_launcher::SocketState;
use spotuify_protocol::{DaemonEvent, DaemonStatus, DoctorReport, UpgradeHint};

pub struct DesktopApp {
    pub(crate) state: DesktopState,
    pub(crate) selected_destination: Destination,
    pub(crate) playback: Option<Playback>,
    pub(crate) update_banner: Option<UpdateBanner>,
    pub(crate) toast: Option<String>,
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
            Self::Search => "Search results land in the next milestone.",
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
                if should_toast_playback_action(&action) {
                    self.toast = Some(format!("Playback updated: {action}"));
                }
            }
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
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
    fn app_shell(&self, state: &ConnectedState, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let mut content = div().flex_1().h_full().flex().flex_col().bg(rgb(0x120f14));

        if let Some(banner) = &self.update_banner {
            content = content.child(update_banner_surface(banner));
        }

        content = content
            .child(self.content_pane(state))
            .child(now_playing_footer(self.playback.as_ref()));

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

fn now_playing_footer(playback: Option<&Playback>) -> impl IntoElement {
    let summary = playback_summary(playback);

    div()
        .h(px(116.))
        .border_t_1()
        .border_color(rgb(0x33252e))
        .bg(rgb(0x19131d))
        .px_6()
        .py_4()
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
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
                .flex()
                .flex_col()
                .items_center()
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(0xf0b78f))
                        .child(summary.state),
                )
                .child(
                    div()
                        .mt_2()
                        .text_xs()
                        .text_color(rgb(0x918698))
                        .child(summary.progress),
                ),
        )
        .child(
            div()
                .text_sm()
                .text_color(rgb(0xb8abbf))
                .child(summary.device)
                .child(
                    div()
                        .mt_1()
                        .text_xs()
                        .text_color(rgb(0x918698))
                        .child(summary.mode),
                ),
        )
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
    let progress = if duration_ms == 0 {
        format_duration(playback.progress_ms)
    } else {
        format!(
            "{} / {}",
            format_duration(playback.progress_ms),
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
    match event {
        DaemonEvent::PlaybackChanged { action, .. } => format!("playback:{action}"),
        DaemonEvent::UpdateAvailable { latest_version, .. } => {
            format!("update-available:{latest_version}")
        }
        DaemonEvent::AuthError { kind } => format!("auth-error:{kind:?}"),
        other => format!("{other:?}"),
    }
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
