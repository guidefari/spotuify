//! Phase 14 (P14-F) — Discord Rich Presence (opt-in).
//!
//! Disabled by default; users add `[discord] enabled = true` + a
//! Discord application_id to flip it on. Failure to connect (no
//! Discord running, app id rejected, IPC socket missing) is logged
//! and disables RPC for the session — never crashes the daemon.

use spotuify_protocol::DaemonEvent;

#[derive(Debug, Clone)]
pub struct DiscordConfig {
    pub enabled: bool,
    pub application_id: String,
}

impl Default for DiscordConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            application_id: String::new(),
        }
    }
}

pub struct DiscordHandle {
    config: DiscordConfig,
}

impl DiscordHandle {
    pub fn new(config: DiscordConfig) -> anyhow::Result<Self> {
        if config.application_id.trim().is_empty() {
            anyhow::bail!("[discord] enabled = true but application_id is empty");
        }
        Ok(Self { config })
    }

    pub fn application_id(&self) -> &str {
        &self.config.application_id
    }

    pub fn enabled(&self) -> bool {
        self.config.enabled
    }

    /// Today this is a no-op stub; the IPC client wire-up lands in a
    /// follow-up once we enrich PlaybackChanged with track metadata.
    /// The handle exists so the SystemIntegration actor compiles
    /// against the same shape regardless of feature flag.
    pub async fn handle(&self, _event: &DaemonEvent) {
        // intentionally empty — see module doc
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_rejects_empty_application_id() {
        // Connecting to Discord without an application_id is a no-op
        // at the IPC layer (the server returns InvalidPayload), so we
        // refuse construction up-front with a clear message rather
        // than spamming the log on every event.
        let cfg = DiscordConfig {
            enabled: true,
            application_id: "  ".into(),
        };
        assert!(DiscordHandle::new(cfg).is_err());
    }

    #[test]
    fn new_accepts_non_empty_application_id() {
        let cfg = DiscordConfig {
            enabled: true,
            application_id: "1234567890".into(),
        };
        assert!(DiscordHandle::new(cfg).is_ok());
    }
}
