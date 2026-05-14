//! Phase 10 (P10.8) — shell-hook executor.
//!
//! Fires a user-configured shell command on every `listen_qualified`
//! event so users can plug spotuify into external scrobblers
//! (ListenBrainz, Last.fm, custom webhooks) without spotuify shipping
//! provider-specific integration in-tree.
//!
//! Environment passed to the hook:
//!
//! - `SPOTUIFY_TRACK_URI`
//! - `SPOTUIFY_DURATION_MS`
//! - `SPOTUIFY_AUDIBLE_MS`
//! - `SPOTUIFY_ARTIST_URI`
//! - `SPOTUIFY_ALBUM_URI`
//!
//! Failure handling is intentionally lenient: the hook runs detached
//! with a hard timeout, errors are logged at `warn`, and the daemon
//! never blocks on it. Misbehaving scrobblers cannot stall playback.

use std::time::Duration;

use tokio::process::Command;
use tokio::time::timeout;

#[derive(Debug, Clone)]
pub struct HookExecutor {
    command: String,
    timeout: Duration,
}

impl HookExecutor {
    pub fn new(command: String, timeout_ms: u64) -> Self {
        Self {
            command,
            timeout: Duration::from_millis(timeout_ms.max(100)),
        }
    }

    /// Fire-and-forget invocation. Returns immediately after spawning;
    /// the actual wait happens in a tokio task so the SessionTracker
    /// hot path doesn't block.
    pub fn fire(
        &self,
        track_uri: &str,
        duration_ms: i64,
        audible_ms: i64,
        artist_uri: Option<&str>,
        album_uri: Option<&str>,
    ) {
        if self.command.trim().is_empty() {
            return;
        }
        let cmd = self.command.clone();
        let to = self.timeout;
        let env: [(String, String); 5] = [
            ("SPOTUIFY_TRACK_URI".to_string(), track_uri.to_string()),
            ("SPOTUIFY_DURATION_MS".to_string(), duration_ms.to_string()),
            ("SPOTUIFY_AUDIBLE_MS".to_string(), audible_ms.to_string()),
            (
                "SPOTUIFY_ARTIST_URI".to_string(),
                artist_uri.unwrap_or("").to_string(),
            ),
            (
                "SPOTUIFY_ALBUM_URI".to_string(),
                album_uri.unwrap_or("").to_string(),
            ),
        ];
        tokio::spawn(async move {
            let mut child = Command::new("sh");
            child.arg("-c").arg(&cmd);
            for (k, v) in &env {
                child.env(k, v);
            }
            child.kill_on_drop(true);
            let result = timeout(to, async {
                child.spawn()?.wait().await.map_err(anyhow::Error::from)
            })
            .await;
            match result {
                Err(_) => tracing::warn!(command = %cmd, "listen-qualified hook timed out"),
                Ok(Err(err)) => {
                    tracing::warn!(error = %err, command = %cmd, "listen-qualified hook failed")
                }
                Ok(Ok(status)) if !status.success() => {
                    tracing::warn!(
                        ?status,
                        command = %cmd,
                        "listen-qualified hook exited non-zero"
                    );
                }
                Ok(Ok(_)) => {}
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_command_is_a_noop() {
        // Should not panic / not spawn.
        let exec = HookExecutor::new("".to_string(), 1_000);
        exec.fire("spotify:track:1", 180_000, 95_000, None, None);
    }

    #[test]
    fn timeout_floor_is_100ms() {
        let exec = HookExecutor::new("true".to_string(), 0);
        assert_eq!(exec.timeout, Duration::from_millis(100));
    }
}
