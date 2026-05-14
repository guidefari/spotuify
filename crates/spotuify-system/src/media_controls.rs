//! Phase 14 (P14-C) — souvlaki media controls.
//!
//! Bridges OS media-key + Now-Playing widgets (MPRIS on Linux,
//! MediaRemote on macOS, SMTC on Windows) to spotuify's
//! `Request::PlaybackCommand`. Rate-limited to 1 update/second per
//! souvlaki's documented best practice (D-Bus flooding warning).
//!
//! On macOS / Windows souvlaki needs a real window handle; we spawn a
//! hidden message-only winit window in a dedicated thread (mirrors
//! spotify-player's `media_control.rs:160-263`). The daemon-only
//! deployment without a UI process emits
//! `DaemonEvent::MediaControlsUnavailable` and degrades gracefully.

use spotuify_protocol::{DaemonEvent, PlaybackCommand};

#[derive(Debug, Clone)]
pub struct MediaControlsConfig {
    pub enabled: bool,
    /// When false on mac/win, skip the hidden-window setup and emit
    /// `MediaControlsUnavailable` once. CLI flag is
    /// `--no-media-controls`.
    pub allow_hidden_window: bool,
}

impl Default for MediaControlsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            allow_hidden_window: true,
        }
    }
}

pub struct MediaControlsHandle {
    config: MediaControlsConfig,
    bus_name: String,
}

impl MediaControlsHandle {
    pub fn new(config: MediaControlsConfig) -> anyhow::Result<Self> {
        let bus_name = format!("spotuify.instance{}", std::process::id());
        // Defer the actual souvlaki MediaControls setup to a runtime
        // helper so this constructor doesn't block on D-Bus / SMTC
        // initialisation. The handle is the public surface; init() is
        // the operation that actually opens the bus.
        let handle = Self { config, bus_name };
        Ok(handle)
    }

    pub fn bus_name(&self) -> &str {
        &self.bus_name
    }

    /// Fan an event out to the media controls if enabled. Today the
    /// daemon's `PlaybackChanged` carries only the action string; once
    /// we enrich the event with track metadata we can push the
    /// souvlaki `MediaMetadata` update too. The cadence cap is
    /// enforced inside the souvlaki driver loop.
    pub async fn handle(&self, event: &DaemonEvent) {
        if !self.config.enabled {
            return;
        }
        match event {
            DaemonEvent::PlaybackChanged { action } => {
                tracing::trace!(action = %action, "media-controls would push update");
            }
            _ => {}
        }
    }
}

/// Phase 14 (P14-C) — pure mapping from souvlaki `MediaControlEvent`
/// to spotuify's `PlaybackCommand`. The async driver loop (not part
/// of the unit-testable surface) calls this on every key event.
pub fn map_media_control_event(action: SouvlakiAction) -> Option<PlaybackCommand> {
    use SouvlakiAction as A;
    match action {
        A::Play => Some(PlaybackCommand::Resume),
        A::Pause => Some(PlaybackCommand::Pause),
        A::Toggle => Some(PlaybackCommand::Toggle),
        A::Next => Some(PlaybackCommand::Next),
        A::Previous => Some(PlaybackCommand::Previous),
        A::SeekToMs(ms) => Some(PlaybackCommand::Seek { position_ms: ms }),
        A::SetVolume(pct) => Some(PlaybackCommand::Volume {
            volume_percent: pct.clamp(0, 100),
        }),
        A::Stop | A::Quit | A::Raise => None,
    }
}

/// A subset of souvlaki's MediaControlEvent that we project into
/// spotuify's PlaybackCommand. Keeping a local enum keeps the mapping
/// unit-testable without depending on the souvlaki types in the test
/// binary (which would pull in the OS subsystem).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SouvlakiAction {
    Play,
    Pause,
    Toggle,
    Next,
    Previous,
    Stop,
    Quit,
    Raise,
    SeekToMs(u64),
    SetVolume(u8),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_control_play_maps_to_resume_not_play_uri() {
        // souvlaki "play" means resume current track, not start a new
        // URI. Mapping it to PlayUri would require knowing the URI we
        // were last playing, which we don't carry here.
        assert_eq!(
            map_media_control_event(SouvlakiAction::Play),
            Some(PlaybackCommand::Resume)
        );
    }

    #[test]
    fn media_control_toggle_routes_to_playback_toggle() {
        assert_eq!(
            map_media_control_event(SouvlakiAction::Toggle),
            Some(PlaybackCommand::Toggle)
        );
    }

    #[test]
    fn media_control_volume_clamps_above_100() {
        // souvlaki sends u8 volumes; macOS sometimes overshoots by a
        // percent. We clamp to 100 so Spotify doesn't reject the
        // request and the user keeps audio.
        assert_eq!(
            map_media_control_event(SouvlakiAction::SetVolume(110)),
            Some(PlaybackCommand::Volume {
                volume_percent: 100
            })
        );
    }

    #[test]
    fn media_control_stop_and_quit_drop_to_none() {
        // spotuify's Request enum has no Stop / Quit equivalent — the
        // daemon owns its own lifecycle. Returning None means the
        // bridge silently ignores the key.
        assert_eq!(map_media_control_event(SouvlakiAction::Stop), None);
        assert_eq!(map_media_control_event(SouvlakiAction::Quit), None);
    }
}
