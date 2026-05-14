use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use spotuify_core::BackendKind;

#[derive(Clone, Debug)]
pub struct Config {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub redirect_uri: String,
    pub config_path: PathBuf,
    pub spotifyd_config_path: PathBuf,
    pub spotifyd_device_name: Option<String>,
    pub spotifyd_autostart: bool,
    pub player: PlayerConfig,
    pub analytics: AnalyticsConfig,
}

/// TOML-side representation of the `[analytics]` section. All fields
/// optional so partial sections work.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub(crate) struct AnalyticsSection {
    #[serde(skip_serializing_if = "Option::is_none")]
    store_raw_queries: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    retention_progress_days: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    retention_events_days: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    retention_operations_days: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    daily_rollup_hour: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hook_command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hook_timeout_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    allow_file_credentials: Option<bool>,
}

/// Phase 10 analytics + Phase 11 headless-Linux flag. Defaults match
/// blueprint values; users can override per-key via TOML.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnalyticsConfig {
    /// When false, raw search queries are dropped on persistence
    /// (only the normalised query hash is kept). Default: true.
    pub store_raw_queries: bool,
    /// Days to retain raw `playback_progress` samples before prune.
    pub retention_progress_days: u32,
    /// Days to retain `analytics_events` before prune.
    pub retention_events_days: u32,
    /// Days to retain `operations` rows before prune.
    pub retention_operations_days: u32,
    /// Local hour (0..=23) at which the daily habit rollup runs.
    pub daily_rollup_hour: u8,
    /// Optional shell command fired on `listen_qualified` events;
    /// bridges to ListenBrainz / Last.fm / Discord recipes.
    pub hook_command: Option<String>,
    /// Hard timeout on `hook_command` execution to keep the daemon
    /// from blocking on a misbehaving scrobbler.
    pub hook_timeout_ms: u64,
    /// Phase 11 headless-Linux opt-in: when true and Secret Service
    /// is unavailable, fall back to an age-encrypted credentials file.
    pub allow_file_credentials: bool,
}

impl Default for AnalyticsConfig {
    fn default() -> Self {
        Self {
            store_raw_queries: true,
            retention_progress_days: 90,
            retention_events_days: 365,
            retention_operations_days: 90,
            daily_rollup_hour: 3,
            hook_command: None,
            hook_timeout_ms: 5_000,
            allow_file_credentials: false,
        }
    }
}

impl AnalyticsConfig {
    pub(crate) fn from_file(file: &FileConfig) -> Self {
        let section = file.analytics.clone().unwrap_or_default();
        let defaults = Self::default();
        Self {
            store_raw_queries: section
                .store_raw_queries
                .unwrap_or(defaults.store_raw_queries),
            retention_progress_days: section
                .retention_progress_days
                .unwrap_or(defaults.retention_progress_days),
            retention_events_days: section
                .retention_events_days
                .unwrap_or(defaults.retention_events_days),
            retention_operations_days: section
                .retention_operations_days
                .unwrap_or(defaults.retention_operations_days),
            daily_rollup_hour: section
                .daily_rollup_hour
                .filter(|h| *h <= 23)
                .unwrap_or(defaults.daily_rollup_hour),
            hook_command: blank_to_none(section.hook_command),
            hook_timeout_ms: section.hook_timeout_ms.unwrap_or(defaults.hook_timeout_ms),
            allow_file_credentials: section
                .allow_file_credentials
                .unwrap_or(defaults.allow_file_credentials),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct FileConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_secret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    redirect_uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    spotifyd: Option<SpotifydConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    player: Option<PlayerSection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    analytics: Option<AnalyticsSection>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default)]
struct SpotifydConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    config_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    device_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    autostart: Option<bool>,
}

/// TOML-side representation of the `[player]` section. All fields are
/// Optional so the section can be partially specified; defaults apply
/// in `PlayerConfig::from_file`.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub(crate) struct PlayerSection {
    #[serde(skip_serializing_if = "Option::is_none")]
    backend: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bitrate: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    device_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    normalization: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    audio_cache_mib: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pulse_props: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    event_hook: Option<String>,
}

/// Fully-resolved `[player]` config with defaults filled in. The
/// daemon, CLI, and player crate all consume this shape.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerConfig {
    pub backend: BackendKind,
    pub bitrate: u32,
    pub device_name: Option<String>,
    pub normalization: bool,
    pub audio_cache_mib: u32,
    pub pulse_props: bool,
    pub event_hook: Option<String>,
}

impl Default for PlayerConfig {
    fn default() -> Self {
        Self {
            backend: BackendKind::default(),
            bitrate: 320,
            device_name: None,
            normalization: false,
            audio_cache_mib: 0,
            pulse_props: true,
            event_hook: None,
        }
    }
}

impl PlayerConfig {
    /// Lift a `[player]` section into a fully-defaulted PlayerConfig.
    /// Falls back to all defaults if the section is missing entirely.
    /// Invalid values are *not* rejected here (use `validate` for
    /// load-time checks); they degrade silently to defaults so a typo
    /// in `event_hook` can't brick the daemon.
    pub(crate) fn from_file(file: &FileConfig) -> Self {
        let section = file.player.clone().unwrap_or_default();
        let backend = section
            .backend
            .as_deref()
            .and_then(|raw| BackendKind::parse(raw).ok())
            .unwrap_or_default();
        let bitrate = section
            .bitrate
            .filter(|b| matches!(b, 96 | 160 | 320))
            .unwrap_or(320);
        Self {
            backend,
            bitrate,
            device_name: blank_to_none(section.device_name),
            normalization: section.normalization.unwrap_or(false),
            audio_cache_mib: section.audio_cache_mib.unwrap_or(0),
            pulse_props: section.pulse_props.unwrap_or(true),
            event_hook: blank_to_none(section.event_hook),
        }
    }

    /// Validate a `[player]` section without mutating state. Returns
    /// the first error encountered — used by `Config::load` so users
    /// see config bugs at startup rather than as silent fallbacks.
    pub(crate) fn validate(file: &FileConfig) -> Result<()> {
        let Some(section) = file.player.as_ref() else {
            return Ok(());
        };
        if let Some(raw) = section.backend.as_deref() {
            BackendKind::parse(raw)
                .map_err(|err| anyhow!("config player.backend invalid: {err}"))?;
        }
        if let Some(bitrate) = section.bitrate {
            if !matches!(bitrate, 96 | 160 | 320) {
                bail!("config player.bitrate invalid: {bitrate} (expected one of 96, 160, 320)");
            }
        }
        Ok(())
    }
}

impl From<PlayerConfig> for PlayerSection {
    fn from(value: PlayerConfig) -> Self {
        Self {
            backend: Some(value.backend.label().to_string()),
            bitrate: Some(value.bitrate),
            device_name: value.device_name,
            normalization: Some(value.normalization),
            audio_cache_mib: Some(value.audio_cache_mib),
            pulse_props: Some(value.pulse_props),
            event_hook: value.event_hook,
        }
    }
}

impl Default for FileConfig {
    fn default() -> Self {
        Self {
            client_id: None,
            client_secret: None,
            redirect_uri: None,
            spotifyd: Some(SpotifydConfig {
                config_path: None,
                device_name: None,
                autostart: Some(true),
            }),
            player: None,
            analytics: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigKey {
    ClientId,
    ClientSecret,
    RedirectUri,
    SpotifydConfigPath,
    SpotifydDeviceName,
    SpotifydAutostart,
    // Phase 9 — player backend.
    PlayerBackend,
    PlayerBitrate,
    PlayerDeviceName,
    PlayerNormalization,
    PlayerAudioCacheMib,
    PlayerPulseProps,
    PlayerEventHook,
}

impl ConfigKey {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "client_id" | "client-id" => Ok(Self::ClientId),
            "client_secret" | "client-secret" => Ok(Self::ClientSecret),
            "redirect_uri" | "redirect-uri" => Ok(Self::RedirectUri),
            "spotifyd.config_path" | "spotifyd.config-path" => Ok(Self::SpotifydConfigPath),
            "spotifyd.device_name" | "spotifyd.device-name" => Ok(Self::SpotifydDeviceName),
            "spotifyd.autostart" => Ok(Self::SpotifydAutostart),
            "player.backend" => Ok(Self::PlayerBackend),
            "player.bitrate" => Ok(Self::PlayerBitrate),
            "player.device_name" | "player.device-name" => Ok(Self::PlayerDeviceName),
            "player.normalization" => Ok(Self::PlayerNormalization),
            "player.audio_cache_mib" | "player.audio-cache-mib" => Ok(Self::PlayerAudioCacheMib),
            "player.pulse_props" | "player.pulse-props" => Ok(Self::PlayerPulseProps),
            "player.event_hook" | "player.event-hook" => Ok(Self::PlayerEventHook),
            _ => bail!(
                "unknown config key `{value}`; expected one of: {}",
                Self::valid_keys().join(", ")
            ),
        }
    }

    pub fn valid_keys() -> &'static [&'static str] {
        &[
            "client_id",
            "client_secret",
            "redirect_uri",
            "spotifyd.config_path",
            "spotifyd.device_name",
            "spotifyd.autostart",
            "player.backend",
            "player.bitrate",
            "player.device_name",
            "player.normalization",
            "player.audio_cache_mib",
            "player.pulse_props",
            "player.event_hook",
        ]
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let config_path = config_path()?;
        ensure_config_exists(&config_path)?;

        let file = read_config_file(&config_path)?;
        PlayerConfig::validate(&file)
            .with_context(|| format!("invalid [player] section in {}", config_path.display()))?;
        let player = PlayerConfig::from_file(&file);
        let analytics = AnalyticsConfig::from_file(&file);

        let client_id = std::env::var("SPOTUIFY_CLIENT_ID")
            .ok()
            .or(file.client_id)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| anyhow!("client_id missing in {}", config_path.display()))?;
        let client_secret = std::env::var("SPOTUIFY_CLIENT_SECRET")
            .ok()
            .or(file.client_secret)
            .filter(|value| !value.trim().is_empty());
        let redirect_uri = std::env::var("SPOTUIFY_REDIRECT_URI")
            .ok()
            .or(file.redirect_uri)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(default_redirect_uri);

        let spotifyd = file.spotifyd;
        let spotifyd_config_path = spotifyd
            .as_ref()
            .and_then(|spotifyd| spotifyd.config_path.as_deref())
            .filter(|value| !value.trim().is_empty())
            .map(expand_home)
            .unwrap_or_else(default_spotifyd_config_path);
        let spotifyd_device_name = spotifyd
            .as_ref()
            .and_then(|spotifyd| spotifyd.device_name.clone())
            .filter(|value| !value.trim().is_empty());
        let spotifyd_autostart = spotifyd
            .and_then(|spotifyd| spotifyd.autostart)
            .unwrap_or(true);

        Ok(Self {
            client_id,
            client_secret,
            redirect_uri,
            config_path,
            spotifyd_config_path,
            spotifyd_device_name,
            spotifyd_autostart,
            player,
            analytics,
        })
    }

    pub fn redacted_client_id(&self) -> String {
        let len = self.client_id.chars().count();
        if len <= 8 {
            return "present".to_string();
        }

        let start: String = self.client_id.chars().take(4).collect();
        let end: String = self
            .client_id
            .chars()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        format!("{start}...{end}")
    }
}

pub fn config_path() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("SPOTUIFY_CONFIG") {
        return Ok(PathBuf::from(path));
    }

    dirs::config_dir()
        .or_else(|| dirs::home_dir().map(|home| home.join(".config")))
        .map(|dir| dir.join("spotuify/spotuify.toml"))
        .ok_or_else(|| anyhow!("could not resolve config directory"))
}

pub fn init_config() -> Result<PathBuf> {
    let path = config_path()?;
    if !path.exists() {
        write_template(&path)?;
    }
    Ok(path)
}

pub fn get_config_value(key: ConfigKey) -> Result<Option<String>> {
    let path = config_path()?;
    let file = if path.exists() {
        read_config_file(&path)?
    } else {
        FileConfig::default()
    };

    let resolved = PlayerConfig::from_file(&file);

    Ok(match key {
        ConfigKey::ClientId => blank_to_none(file.client_id),
        ConfigKey::ClientSecret => blank_to_none(file.client_secret),
        ConfigKey::RedirectUri => {
            blank_to_none(file.redirect_uri).or_else(|| Some(default_redirect_uri()))
        }
        ConfigKey::SpotifydConfigPath => file
            .spotifyd
            .as_ref()
            .and_then(|spotifyd| blank_to_none(spotifyd.config_path.clone()))
            .or_else(|| Some(default_spotifyd_config_path().display().to_string())),
        ConfigKey::SpotifydDeviceName => file
            .spotifyd
            .as_ref()
            .and_then(|spotifyd| blank_to_none(spotifyd.device_name.clone())),
        ConfigKey::SpotifydAutostart => Some(
            file.spotifyd
                .and_then(|spotifyd| spotifyd.autostart)
                .unwrap_or(true)
                .to_string(),
        ),
        ConfigKey::PlayerBackend => Some(resolved.backend.label().to_string()),
        ConfigKey::PlayerBitrate => Some(resolved.bitrate.to_string()),
        ConfigKey::PlayerDeviceName => resolved.device_name,
        ConfigKey::PlayerNormalization => Some(resolved.normalization.to_string()),
        ConfigKey::PlayerAudioCacheMib => Some(resolved.audio_cache_mib.to_string()),
        ConfigKey::PlayerPulseProps => Some(resolved.pulse_props.to_string()),
        ConfigKey::PlayerEventHook => resolved.event_hook,
    })
}

pub fn set_config_value(key: ConfigKey, value: &str) -> Result<PathBuf> {
    let path = init_config()?;
    let mut file = read_config_file(&path)?;

    match key {
        ConfigKey::ClientId => file.client_id = blank_to_none(Some(value.to_string())),
        ConfigKey::ClientSecret => file.client_secret = blank_to_none(Some(value.to_string())),
        ConfigKey::RedirectUri => file.redirect_uri = blank_to_none(Some(value.to_string())),
        ConfigKey::SpotifydConfigPath => {
            spotifyd_config_mut(&mut file).config_path = blank_to_none(Some(value.to_string()));
        }
        ConfigKey::SpotifydDeviceName => {
            spotifyd_config_mut(&mut file).device_name = blank_to_none(Some(value.to_string()));
        }
        ConfigKey::SpotifydAutostart => {
            spotifyd_config_mut(&mut file).autostart = Some(parse_bool(value)?);
        }
        ConfigKey::PlayerBackend => {
            let parsed = BackendKind::parse(value)
                .map_err(|err| anyhow!("invalid value for player.backend: {err}"))?;
            player_section_mut(&mut file).backend = Some(parsed.label().to_string());
        }
        ConfigKey::PlayerBitrate => {
            let parsed: u32 = value.trim().parse().with_context(|| {
                format!("expected an integer for player.bitrate, got `{value}`")
            })?;
            if !matches!(parsed, 96 | 160 | 320) {
                bail!("player.bitrate must be one of 96, 160, 320 (got `{parsed}`)");
            }
            player_section_mut(&mut file).bitrate = Some(parsed);
        }
        ConfigKey::PlayerDeviceName => {
            player_section_mut(&mut file).device_name = blank_to_none(Some(value.to_string()));
        }
        ConfigKey::PlayerNormalization => {
            player_section_mut(&mut file).normalization = Some(parse_bool(value)?);
        }
        ConfigKey::PlayerAudioCacheMib => {
            let parsed: u32 = value.trim().parse().with_context(|| {
                format!("expected a non-negative integer for player.audio_cache_mib, got `{value}`")
            })?;
            player_section_mut(&mut file).audio_cache_mib = Some(parsed);
        }
        ConfigKey::PlayerPulseProps => {
            player_section_mut(&mut file).pulse_props = Some(parse_bool(value)?);
        }
        ConfigKey::PlayerEventHook => {
            player_section_mut(&mut file).event_hook = blank_to_none(Some(value.to_string()));
        }
    }

    write_config_file(&path, &file)?;
    Ok(path)
}

fn ensure_config_exists(path: &Path) -> Result<()> {
    if path.exists() {
        return Ok(());
    }

    write_template(path)?;
    bail!(
        "created {}; add your Spotify client_id and client_secret, then rerun spotuify",
        path.display()
    )
}

fn read_config_file(path: &Path) -> Result<FileConfig> {
    let contents =
        fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
    toml::from_str(&contents).with_context(|| format!("could not parse {}", path.display()))
}

fn write_config_file(path: &Path, file: &FileConfig) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let contents = toml::to_string_pretty(file).context("failed to encode config")?;
    fs::write(path, contents).with_context(|| format!("failed to write {}", path.display()))
}

fn write_template(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(path, CONFIG_TEMPLATE).with_context(|| format!("failed to create {}", path.display()))
}

fn spotifyd_config_mut(file: &mut FileConfig) -> &mut SpotifydConfig {
    file.spotifyd.get_or_insert_with(SpotifydConfig::default)
}

fn player_section_mut(file: &mut FileConfig) -> &mut PlayerSection {
    file.player.get_or_insert_with(PlayerSection::default)
}

fn default_redirect_uri() -> String {
    "http://127.0.0.1:8888/callback".to_string()
}

fn blank_to_none(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_string();
        if value.is_empty() {
            None
        } else {
            Some(value)
        }
    })
}

fn parse_bool(value: &str) -> Result<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        _ => bail!("expected true or false, got `{value}`"),
    }
}

fn default_spotifyd_config_path() -> PathBuf {
    dirs::config_dir()
        .or_else(|| dirs::home_dir().map(|home| home.join(".config")))
        .map(|dir| dir.join("spotifyd/spotifyd.conf"))
        .unwrap_or_else(|| PathBuf::from("spotifyd.conf"))
}

fn expand_home(value: &str) -> PathBuf {
    if let Some(rest) = value.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(value)
}

const CONFIG_TEMPLATE: &str = r#"# spotuify config
# Copy your Spotify app credentials from https://developer.spotify.com/dashboard.
client_id = ""
client_secret = ""
redirect_uri = "http://127.0.0.1:8888/callback"

[spotifyd]
autostart = true
# Set this if your spotifyd config lives outside ~/.config/spotifyd/spotifyd.conf.
# config_path = "~/.config/spotifyd/spotifyd.conf"
# device_name = "spotuify"

[player]
# Which backend registers a Spotify Connect device:
#   "embedded" — in-process librespot (Phase 9, requires Premium)
#   "spotifyd" — supervised sibling process (default during rollout)
#   "connect"  — no local device, remote-control existing Connect devices
backend = "spotifyd"
# Stream quality. One of 96, 160, 320. Embedded only.
bitrate = 320
# Optional: override the Connect device name. Defaults to the hostname.
# device_name = "spotuify"
# ReplayGain normalization. Embedded only.
normalization = false
# Disk cache for audio frames in MiB; 0 disables caching.
audio_cache_mib = 0
# Set PULSE_PROP_* env vars so spotuify appears nicely in pavucontrol (Linux only).
pulse_props = true
# Optional shell command run on each PlayerEvent (Unix-style extensibility).
# event_hook = "/usr/local/bin/notify"
"#;

#[cfg(test)]
mod tests {
    use super::{expand_home, parse_bool};

    #[test]
    fn keeps_absolute_paths() {
        assert_eq!(
            expand_home("/tmp/spotifyd.conf"),
            std::path::PathBuf::from("/tmp/spotifyd.conf")
        );
    }

    #[test]
    fn parses_bool_config_values() {
        assert!(parse_bool("on").unwrap());
        assert!(!parse_bool("false").unwrap());
        assert!(parse_bool("later").is_err());
    }
}

// ---------- Phase 9 — [player] config tests (red phase) ----------
//
// Asserts each new field's default *value* (not Default::default()
// self-equality), validates bitrate is one of {96, 160, 320},
// rejects unknown backend kinds with the typo present in the error,
// round-trips a fully-populated [player] section, and confirms the
// ConfigKey parse + setter path covers every new key.
#[cfg(test)]
mod player_config {
    use super::{AnalyticsConfig, ConfigKey, FileConfig, PlayerConfig};
    use spotuify_core::BackendKind;

    #[test]
    fn empty_toml_yields_explicit_defaults() {
        let file: FileConfig = toml::from_str("").unwrap();
        let player = PlayerConfig::from_file(&file);

        assert_eq!(
            player.backend,
            BackendKind::Spotifyd,
            "default backend preserves pre-phase-9 behaviour"
        );
        assert_eq!(player.bitrate, 320, "default bitrate is the highest tier");
        assert_eq!(
            player.device_name, None,
            "no device_name means use hostname"
        );
        assert!(!player.normalization, "ReplayGain off by default");
        assert_eq!(player.audio_cache_mib, 0, "audio cache disabled by default");
        assert!(player.pulse_props, "pulse_props on by default (Linux only)");
        assert_eq!(player.event_hook, None);
    }

    #[test]
    fn populated_player_section_parses_every_field() {
        let toml = r#"
[player]
backend = "embedded"
bitrate = 160
device_name = "studio"
normalization = true
audio_cache_mib = 256
pulse_props = false
event_hook = "/usr/local/bin/notify"
"#;
        let file: FileConfig = toml::from_str(toml).unwrap();
        let player = PlayerConfig::from_file(&file);

        assert_eq!(player.backend, BackendKind::Embedded);
        assert_eq!(player.bitrate, 160);
        assert_eq!(player.device_name.as_deref(), Some("studio"));
        assert!(player.normalization);
        assert_eq!(player.audio_cache_mib, 256);
        assert!(!player.pulse_props);
        assert_eq!(player.event_hook.as_deref(), Some("/usr/local/bin/notify"));
    }

    #[test]
    fn bitrate_outside_known_tiers_is_rejected() {
        // Adversarial: 200 is plausible-looking but invalid. Catches the
        // bug where the parser silently accepts any u32.
        let toml = r#"
[player]
bitrate = 200
"#;
        let file: FileConfig = toml::from_str(toml).unwrap();
        let err = PlayerConfig::validate(&file).expect_err("bitrate=200 must error");
        assert!(err.to_string().contains("200"), "err: {err}");
        assert!(err.to_string().contains("bitrate"), "err: {err}");
    }

    #[test]
    fn backend_typo_surfaces_the_typo_in_the_error() {
        // Adversarial: error must echo what the user typed so they can
        // fix the line without grepping. A generic "invalid backend"
        // message would fail this.
        let toml = r#"
[player]
backend = "embeded"
"#;
        let file: FileConfig = toml::from_str(toml).unwrap();
        let err = PlayerConfig::validate(&file).expect_err("typo must error");
        assert!(
            err.to_string().contains("embeded"),
            "err message must echo the typo `embeded`, got: {err}"
        );
    }

    #[test]
    fn defaults_round_trip_through_toml() {
        // Adversarial: catches the bug where adding a field forgets
        // `#[serde(default)]` — round-trip would lose the value.
        let original = PlayerConfig {
            backend: BackendKind::Embedded,
            bitrate: 96,
            device_name: Some("kitchen".to_string()),
            normalization: true,
            audio_cache_mib: 128,
            pulse_props: false,
            event_hook: Some("hook.sh".to_string()),
        };
        let serialized = toml::to_string_pretty(&FileConfig {
            client_id: None,
            client_secret: None,
            redirect_uri: None,
            spotifyd: None,
            player: Some(original.clone().into()),
            analytics: None,
        })
        .unwrap();
        let parsed: FileConfig = toml::from_str(&serialized).unwrap();
        let round_tripped = PlayerConfig::from_file(&parsed);

        assert_eq!(round_tripped, original);
    }

    #[test]
    fn config_key_parses_every_player_key() {
        assert_eq!(
            ConfigKey::parse("player.backend").unwrap(),
            ConfigKey::PlayerBackend
        );
        assert_eq!(
            ConfigKey::parse("player.bitrate").unwrap(),
            ConfigKey::PlayerBitrate
        );
        assert_eq!(
            ConfigKey::parse("player.device_name").unwrap(),
            ConfigKey::PlayerDeviceName
        );
        assert_eq!(
            ConfigKey::parse("player.device-name").unwrap(),
            ConfigKey::PlayerDeviceName
        );
        assert_eq!(
            ConfigKey::parse("player.normalization").unwrap(),
            ConfigKey::PlayerNormalization
        );
        assert_eq!(
            ConfigKey::parse("player.audio_cache_mib").unwrap(),
            ConfigKey::PlayerAudioCacheMib
        );
        assert_eq!(
            ConfigKey::parse("player.pulse_props").unwrap(),
            ConfigKey::PlayerPulseProps
        );
        assert_eq!(
            ConfigKey::parse("player.event_hook").unwrap(),
            ConfigKey::PlayerEventHook
        );
    }

    #[test]
    fn config_key_valid_keys_lists_every_player_field() {
        // Adversarial: the error message in ConfigKey::parse is the
        // only discoverability surface for users. Locking the listing
        // catches the bug where someone adds a key but forgets the
        // help text.
        let valid = ConfigKey::valid_keys();
        for key in &[
            "player.backend",
            "player.bitrate",
            "player.device_name",
            "player.normalization",
            "player.audio_cache_mib",
            "player.pulse_props",
            "player.event_hook",
        ] {
            assert!(
                valid.contains(key),
                "valid_keys missing {key}; got {valid:?}"
            );
        }
    }

    #[test]
    fn analytics_config_defaults_match_blueprint() {
        let cfg = AnalyticsConfig::default();
        assert!(cfg.store_raw_queries);
        assert_eq!(cfg.retention_progress_days, 90);
        assert_eq!(cfg.retention_events_days, 365);
        assert_eq!(cfg.retention_operations_days, 90);
        assert_eq!(cfg.daily_rollup_hour, 3);
        assert!(cfg.hook_command.is_none());
        assert_eq!(cfg.hook_timeout_ms, 5_000);
        assert!(!cfg.allow_file_credentials);
    }

    #[test]
    fn analytics_section_from_partial_toml_keeps_defaults_for_missing_keys() {
        let toml = r#"
[analytics]
store_raw_queries = false
hook_command = "scrobble.sh"
"#;
        let file: FileConfig = toml::from_str(toml).unwrap();
        let cfg = AnalyticsConfig::from_file(&file);
        assert!(!cfg.store_raw_queries);
        assert_eq!(cfg.hook_command.as_deref(), Some("scrobble.sh"));
        // Unset fields fall back to defaults:
        assert_eq!(cfg.retention_progress_days, 90);
        assert_eq!(cfg.daily_rollup_hour, 3);
    }

    #[test]
    fn analytics_daily_rollup_hour_out_of_range_falls_back_to_default() {
        let toml = "[analytics]\ndaily_rollup_hour = 25\n";
        let file: FileConfig = toml::from_str(toml).unwrap();
        let cfg = AnalyticsConfig::from_file(&file);
        assert_eq!(cfg.daily_rollup_hour, 3);
    }
}
