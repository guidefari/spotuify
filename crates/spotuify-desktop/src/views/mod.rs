use gpui::prelude::*;
use gpui::{
    div, px, rgb, Context, DragMoveEvent, IntoElement, MouseButton, Render, SharedString, Window,
};
use spotuify_core::{MediaItem, Playback};
use spotuify_launcher::SocketState;
use spotuify_protocol::{
    DaemonEvent, DaemonStatus, DoctorReport, PlaybackCommand, Request, UpgradeHint,
};
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
    slider_drag: Option<SliderKind>,
    slider_preview: Option<SliderPreview>,
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
            command_tx: None,
            slider_tx: None,
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

    fn send_playback_command(&mut self, command: PlaybackCommand) {
        let Some(command_tx) = &self.command_tx else {
            self.toast = Some("Transport is not connected to the daemon".to_string());
            return;
        };

        if command_tx
            .send(Request::PlaybackCommand { command })
            .is_err()
        {
            self.toast = Some("Transport connection closed".to_string());
        }
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
    fn app_shell(&self, state: &ConnectedState, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let mut content = div().flex_1().h_full().flex().flex_col().bg(rgb(0x120f14));

        if let Some(banner) = &self.update_banner {
            content = content.child(update_banner_surface(banner));
        }

        content = content
            .child(self.content_pane(state))
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
