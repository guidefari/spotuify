//! Phase 14 (P14-E) — shell-hook event dispatcher.
//!
//! Adopts spotify-player's positional-arg protocol plus env vars for
//! richer fields. Each `DaemonEvent` we forward to the user's hook
//! command becomes one process invocation:
//!
//! ```text
//! <cmd> track-change <uri> <name> <artist> <album> <duration_ms>
//! <cmd> playback-paused <uri> <position_ms>
//! <cmd> playback-resumed <uri> <position_ms>
//! <cmd> track-finished <uri> <reason>
//! <cmd> listen-qualified <uri> <duration_ms>
//! ```
//!
//! Plus `SPOTUIFY_*` env vars so power-user hooks can ignore positional
//! args and just `echo $SPOTUIFY_TRACK > /tmp/now-playing.txt`.
//!
//! Failures (timeout, non-zero exit, missing binary) are logged but
//! never block the daemon — hooks are best-effort by design.

use std::ffi::OsStr;
use std::time::Duration;

use spotuify_protocol::DaemonEvent;

#[derive(Debug, Clone)]
pub struct HookConfig {
    pub hook_command: String,
    pub timeout_ms: u64,
}

impl Default for HookConfig {
    fn default() -> Self {
        Self {
            hook_command: String::new(),
            timeout_ms: 5_000,
        }
    }
}

/// One typed event the dispatcher knows how to fire. The protocol's
/// `DaemonEvent` is wider; we project the ones a user-facing hook can
/// usefully act on into [`HookEvent`].
#[derive(Debug, Clone)]
pub enum HookEvent {
    TrackChange {
        uri: String,
        track: String,
        artist: String,
        album: String,
        duration_ms: u64,
    },
    PlaybackPaused {
        uri: String,
        position_ms: u64,
    },
    PlaybackResumed {
        uri: String,
        position_ms: u64,
    },
    TrackFinished {
        uri: String,
        reason: String,
    },
    ListenQualified {
        uri: String,
        duration_ms: i64,
    },
}

impl HookEvent {
    fn argv(&self) -> Vec<String> {
        match self {
            Self::TrackChange {
                uri,
                track,
                artist,
                album,
                duration_ms,
            } => vec![
                "track-change".into(),
                uri.clone(),
                track.clone(),
                artist.clone(),
                album.clone(),
                duration_ms.to_string(),
            ],
            Self::PlaybackPaused { uri, position_ms } => vec![
                "playback-paused".into(),
                uri.clone(),
                position_ms.to_string(),
            ],
            Self::PlaybackResumed { uri, position_ms } => vec![
                "playback-resumed".into(),
                uri.clone(),
                position_ms.to_string(),
            ],
            Self::TrackFinished { uri, reason } => {
                vec!["track-finished".into(), uri.clone(), reason.clone()]
            }
            Self::ListenQualified { uri, duration_ms } => vec![
                "listen-qualified".into(),
                uri.clone(),
                duration_ms.to_string(),
            ],
        }
    }

    fn env(&self) -> Vec<(&'static str, String)> {
        let event = match self {
            Self::TrackChange { .. } => "track-change",
            Self::PlaybackPaused { .. } => "playback-paused",
            Self::PlaybackResumed { .. } => "playback-resumed",
            Self::TrackFinished { .. } => "track-finished",
            Self::ListenQualified { .. } => "listen-qualified",
        };
        let mut env: Vec<(&'static str, String)> = vec![("SPOTUIFY_EVENT", event.to_string())];
        match self {
            Self::TrackChange {
                uri,
                track,
                artist,
                album,
                duration_ms,
            } => {
                env.push(("SPOTUIFY_URI", uri.clone()));
                env.push(("SPOTUIFY_TRACK", track.clone()));
                env.push(("SPOTUIFY_ARTIST", artist.clone()));
                env.push(("SPOTUIFY_ALBUM", album.clone()));
                env.push(("SPOTUIFY_DURATION_MS", duration_ms.to_string()));
            }
            Self::PlaybackPaused { uri, position_ms }
            | Self::PlaybackResumed { uri, position_ms } => {
                env.push(("SPOTUIFY_URI", uri.clone()));
                env.push(("SPOTUIFY_POSITION_MS", position_ms.to_string()));
            }
            Self::TrackFinished { uri, reason } => {
                env.push(("SPOTUIFY_URI", uri.clone()));
                env.push(("SPOTUIFY_REASON", reason.clone()));
            }
            Self::ListenQualified { uri, duration_ms } => {
                env.push(("SPOTUIFY_URI", uri.clone()));
                env.push(("SPOTUIFY_DURATION_MS", duration_ms.to_string()));
            }
        }
        env
    }
}

#[derive(Clone)]
pub struct HookDispatcher {
    config: HookConfig,
}

impl HookDispatcher {
    pub fn new(config: HookConfig) -> Self {
        Self { config }
    }

    /// Spawn the user's hook for `event`. Never blocks longer than
    /// `config.timeout_ms`; logs and drops non-zero exit + timeout
    /// without bubbling to the daemon.
    pub async fn fire(&self, event: HookEvent) -> anyhow::Result<()> {
        if self.config.hook_command.trim().is_empty() {
            return Ok(());
        }
        let argv = event.argv();
        let env = event.env();
        let command = self.config.hook_command.clone();
        let timeout = Duration::from_millis(self.config.timeout_ms.max(100));
        let (program, base_args) = split_command(&command);
        let mut cmd = tokio::process::Command::new(program);
        for a in base_args {
            cmd.arg(a);
        }
        for a in &argv {
            cmd.arg(OsStr::new(a));
        }
        for (k, v) in &env {
            cmd.env(k, v);
        }
        cmd.kill_on_drop(true);
        let started = std::time::Instant::now();
        let child = cmd.spawn();
        match child {
            Ok(child) => {
                let outcome = tokio::time::timeout(timeout, wait_child(child)).await;
                match outcome {
                    Ok(Ok(status)) if status.success() => Ok(()),
                    Ok(Ok(status)) => {
                        tracing::warn!(
                            elapsed_ms = started.elapsed().as_millis(),
                            exit = ?status.code(),
                            "hook exited non-zero"
                        );
                        Ok(())
                    }
                    Ok(Err(err)) => {
                        tracing::warn!(error = %err, "hook process error");
                        Ok(())
                    }
                    Err(_) => {
                        tracing::warn!(
                            timeout_ms = self.config.timeout_ms,
                            "hook timed out; killed"
                        );
                        Ok(())
                    }
                }
            }
            Err(err) => {
                tracing::warn!(error = %err, command = %command, "failed to spawn hook");
                Ok(())
            }
        }
    }

    /// Bridge `DaemonEvent → HookEvent → fire`. Not every daemon event
    /// has a hook projection; those return Ok(()) without spawning.
    pub async fn handle(&self, event: &DaemonEvent) -> anyhow::Result<()> {
        let projected = project(event);
        if let Some(hook_event) = projected {
            self.fire(hook_event).await?;
        }
        Ok(())
    }
}

async fn wait_child(mut child: tokio::process::Child) -> std::io::Result<std::process::ExitStatus> {
    child.wait().await
}

fn split_command(raw: &str) -> (String, Vec<String>) {
    let mut parts = raw.split_whitespace();
    let head = parts.next().unwrap_or("").to_string();
    let tail = parts.map(String::from).collect();
    (head, tail)
}

/// Phase 14 (P14-E) — pure projection from `DaemonEvent` to
/// `HookEvent`. Unit-testable. Events that don't have a hook contract
/// (e.g. AuthError, RateLimited) return `None`.
pub fn project(event: &DaemonEvent) -> Option<HookEvent> {
    use DaemonEvent as E;
    match event {
        E::ListenQualified {
            track_uri,
            duration_ms,
            ..
        } => Some(HookEvent::ListenQualified {
            uri: track_uri.clone(),
            duration_ms: *duration_ms,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_change_event_renders_positional_argv_in_spotify_player_compatible_order() {
        let ev = HookEvent::TrackChange {
            uri: "spotify:track:abc".into(),
            track: "Hello".into(),
            artist: "Adele".into(),
            album: "25".into(),
            duration_ms: 220_000,
        };
        assert_eq!(
            ev.argv(),
            vec![
                "track-change".to_string(),
                "spotify:track:abc".to_string(),
                "Hello".to_string(),
                "Adele".to_string(),
                "25".to_string(),
                "220000".to_string(),
            ]
        );
    }

    #[test]
    fn track_change_event_sets_env_vars_for_richer_hooks() {
        let ev = HookEvent::TrackChange {
            uri: "spotify:track:xyz".into(),
            track: "Strobe".into(),
            artist: "Deadmau5".into(),
            album: "For Lack Of A Better Name".into(),
            duration_ms: 600_000,
        };
        let env: std::collections::HashMap<_, _> = ev.env().into_iter().collect();
        assert_eq!(env["SPOTUIFY_EVENT"], "track-change");
        assert_eq!(env["SPOTUIFY_URI"], "spotify:track:xyz");
        assert_eq!(env["SPOTUIFY_TRACK"], "Strobe");
        assert_eq!(env["SPOTUIFY_ARTIST"], "Deadmau5");
        assert_eq!(env["SPOTUIFY_DURATION_MS"], "600000");
    }

    #[test]
    fn project_maps_listen_qualified_to_hook_event() {
        let ev = DaemonEvent::ListenQualified {
            track_uri: "spotify:track:abc".into(),
            duration_ms: 250_000,
            audible_ms: 240_000,
            artist_uri: None,
            album_uri: None,
        };
        match project(&ev) {
            Some(HookEvent::ListenQualified { uri, duration_ms }) => {
                assert_eq!(uri, "spotify:track:abc");
                assert_eq!(duration_ms, 250_000);
            }
            other => panic!("expected ListenQualified, got {other:?}"),
        }
    }

    #[test]
    fn project_returns_none_for_uncontracted_events() {
        // Auth errors aren't routed to user hooks — they're spotuify
        // operational telemetry. The hook contract should stay narrow
        // so users aren't surprised by internal events.
        let ev = DaemonEvent::AuthError {
            kind: spotuify_protocol::AuthErrorKind::ExpiredRefresh,
        };
        assert!(project(&ev).is_none());
    }

    #[test]
    fn split_command_handles_args_after_program() {
        let (prog, args) = split_command("/usr/bin/env python /opt/hook.py");
        assert_eq!(prog, "/usr/bin/env");
        assert_eq!(args, vec!["python", "/opt/hook.py"]);
    }
}
