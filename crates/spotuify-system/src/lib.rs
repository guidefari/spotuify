//! Phase 14 — system integration.
//!
//! Owns the cross-cutting OS-facing concerns:
//! - **Cover-art file cache** (`cover_cache`): TTL-bounded, integrity-gated
//!   download cache used by media-controls + notifications + (future) TUI.
//! - **Media controls** (`media_controls`, opt-in feature `media-controls`):
//!   souvlaki bridge that maps OS media keys / Now-Playing widgets to
//!   `Request::PlaybackCommand`. Rate-limited to 1 update / second.
//! - **Desktop notifications** (`notifications`, feature `notifications`):
//!   notify-rust bridge with per-event toggles and templated bodies.
//! - **Shell hooks** (`hooks`, feature `hooks`): spotify-player-style
//!   `[analytics] hook_command` dispatcher with positional argv + env vars.
//! - **Discord Rich Presence** (`discord`, feature `discord-rpc`): opt-in.
//!
//! The daemon spawns one [`SystemIntegration`] on startup that subscribes
//! to the `DaemonEvent` broadcast and fans events out to whichever
//! subsystems are enabled.

pub mod cover_cache;
pub mod hooks;

#[cfg(feature = "notifications")]
pub mod notifications;

#[cfg(feature = "media-controls")]
pub mod media_controls;

#[cfg(all(feature = "media-controls", target_os = "windows"))]
mod media_controls_win;

#[cfg(feature = "discord-rpc")]
pub mod discord;

pub use cover_cache::{
    CoverCache, CoverCacheConfig, CoverCacheEntry, CoverCacheError, CoverCacheStats,
};
pub use hooks::{HookConfig, HookDispatcher, HookEvent};

use std::sync::Arc;

use spotuify_protocol::{PlaybackCommand, SystemDiagnostics};

/// Phase 14 (P14-G) — the daemon-side actor that fans `DaemonEvent`s
/// out to the configured subsystems. Each subsystem is `Option<>` so
/// disabled or failed initialisations degrade gracefully.
pub struct SystemIntegration {
    pub cover_cache: Arc<CoverCache>,
    pub hooks: Option<HookDispatcher>,
    #[cfg(feature = "notifications")]
    pub notifications: Option<notifications::NotificationsHandle>,
    #[cfg(feature = "media-controls")]
    pub media_controls: Option<media_controls::MediaControlsHandle>,
    #[cfg(feature = "discord-rpc")]
    pub discord: Option<discord::DiscordHandle>,
}

impl SystemIntegration {
    /// Build a [`SystemIntegration`] from config. Each subsystem can
    /// fail independently — the daemon still starts.
    pub fn spawn(config: SystemConfig) -> Self {
        let cover_cache = Arc::new(CoverCache::new(config.cover_cache));
        let hooks = config.hooks.map(HookDispatcher::new);

        #[cfg(feature = "notifications")]
        let notifications = config
            .notifications
            .filter(|cfg| cfg.enabled)
            .and_then(|cfg| match notifications::NotificationsHandle::new(cfg) {
                Ok(handle) => Some(handle),
                Err(err) => {
                    tracing::warn!(error = %err, "notifications subsystem failed to start");
                    None
                }
            });

        #[cfg(feature = "media-controls")]
        let media_controls = config
            .media_controls
            .filter(|cfg| cfg.enabled)
            .and_then(|cfg| match media_controls::MediaControlsHandle::new(cfg) {
                Ok(handle) => Some(handle),
                Err(err) => {
                    tracing::warn!(error = %err, "media-controls subsystem failed to start");
                    None
                }
            });

        #[cfg(feature = "discord-rpc")]
        let discord = config.discord.filter(|cfg| cfg.enabled).and_then(|cfg| {
            match discord::DiscordHandle::new(cfg) {
                Ok(handle) => Some(handle),
                Err(err) => {
                    tracing::warn!(error = %err, "discord-rpc subsystem failed to start");
                    None
                }
            }
        });

        Self {
            cover_cache,
            hooks,
            #[cfg(feature = "notifications")]
            notifications,
            #[cfg(feature = "media-controls")]
            media_controls,
            #[cfg(feature = "discord-rpc")]
            discord,
        }
    }

    /// Route a daemon event to every enabled subsystem.
    pub async fn handle_event(&self, event: &spotuify_protocol::DaemonEvent) {
        if let Some(hooks) = &self.hooks {
            let _ = hooks.handle(event).await;
        }
        #[cfg(feature = "notifications")]
        if let Some(n) = &self.notifications {
            n.handle(event).await;
        }
        #[cfg(feature = "media-controls")]
        if let Some(m) = &self.media_controls {
            m.handle(event).await;
        }
        #[cfg(feature = "discord-rpc")]
        if let Some(d) = &self.discord {
            d.handle(event).await;
        }
    }

    pub fn has_media_controls(&self) -> bool {
        #[cfg(feature = "media-controls")]
        {
            self.media_controls.is_some()
        }
        #[cfg(not(feature = "media-controls"))]
        {
            false
        }
    }

    pub async fn recv_media_control_command(&self) -> Option<PlaybackCommand> {
        #[cfg(feature = "media-controls")]
        {
            match &self.media_controls {
                Some(media_controls) => media_controls.recv_command().await,
                None => None,
            }
        }
        #[cfg(not(feature = "media-controls"))]
        {
            None
        }
    }

    pub fn diagnostics(&self) -> SystemDiagnostics {
        #[cfg(feature = "media-controls")]
        let (media_controls_enabled, media_controls_bus_name) = self
            .media_controls
            .as_ref()
            .map(|media_controls| (true, Some(media_controls.bus_name().to_string())))
            .unwrap_or((false, None));
        #[cfg(not(feature = "media-controls"))]
        let (media_controls_enabled, media_controls_bus_name) = (false, None);

        let (hooks_enabled, hook_command, hook_timeout_ms) =
            self.hooks.as_ref().map_or((false, None, None), |hooks| {
                (
                    !hooks.hook_command().trim().is_empty(),
                    Some(hooks.hook_command().to_string()),
                    Some(hooks.timeout_ms()),
                )
            });

        #[cfg(feature = "notifications")]
        let notifications_enabled = self
            .notifications
            .as_ref()
            .map(|notifications| notifications.enabled())
            .unwrap_or(false);
        #[cfg(not(feature = "notifications"))]
        let notifications_enabled = false;

        #[cfg(feature = "discord-rpc")]
        let (discord_enabled, discord_application_id) = self
            .discord
            .as_ref()
            .map(|discord| {
                (
                    discord.enabled(),
                    Some(discord.application_id().to_string()),
                )
            })
            .unwrap_or((false, None));
        #[cfg(not(feature = "discord-rpc"))]
        let (discord_enabled, discord_application_id) = (false, None);

        SystemDiagnostics {
            media_controls_enabled,
            media_controls_bus_name,
            hooks_enabled,
            hook_command,
            hook_timeout_ms,
            notifications_enabled,
            discord_enabled,
            discord_application_id,
        }
    }
}

/// Aggregated config block fed to [`SystemIntegration::spawn`]. The
/// daemon reads it from `config.toml` and passes individual sub-configs
/// through. Sub-configs are optional so missing TOML sections degrade
/// to "disabled".
#[derive(Debug, Clone, Default)]
pub struct SystemConfig {
    pub cover_cache: CoverCacheConfig,
    pub hooks: Option<HookConfig>,
    #[cfg(feature = "notifications")]
    pub notifications: Option<notifications::NotificationsConfig>,
    #[cfg(feature = "media-controls")]
    pub media_controls: Option<media_controls::MediaControlsConfig>,
    #[cfg(feature = "discord-rpc")]
    pub discord: Option<discord::DiscordConfig>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostics_reports_configured_hook() {
        let system = SystemIntegration::spawn(SystemConfig {
            hooks: Some(HookConfig {
                hook_command: "scrobble.sh".to_string(),
                timeout_ms: 1_500,
            }),
            ..SystemConfig::default()
        });

        let diagnostics = system.diagnostics();

        assert!(diagnostics.hooks_enabled);
        assert_eq!(diagnostics.hook_command.as_deref(), Some("scrobble.sh"));
        assert_eq!(diagnostics.hook_timeout_ms, Some(1_500));
    }
}
