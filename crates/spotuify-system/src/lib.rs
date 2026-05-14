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
//!   `[events] hook_command` dispatcher with positional argv + env vars.
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

#[cfg(feature = "discord-rpc")]
pub mod discord;

pub use cover_cache::{CoverCache, CoverCacheConfig, CoverCacheError};
pub use hooks::{HookConfig, HookDispatcher, HookEvent};

use std::sync::Arc;

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
        let hooks = if let Some(cfg) = config.hooks {
            Some(HookDispatcher::new(cfg))
        } else {
            None
        };

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
        let discord = config
            .discord
            .filter(|cfg| cfg.enabled)
            .and_then(|cfg| match discord::DiscordHandle::new(cfg) {
                Ok(handle) => Some(handle),
                Err(err) => {
                    tracing::warn!(error = %err, "discord-rpc subsystem failed to start");
                    None
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
