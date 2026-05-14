//! Core domain types for spotuify.
//!
//! Per `docs/blueprint/01-architecture.md` §"Dependency rules", this crate has
//! **no internal dependencies**. Every other workspace member may import from
//! it; it imports from nothing in the workspace.
//!
//! These types describe the music domain — what plays, what's queued, what
//! devices exist, what playlists hold. IPC framing, HTTP semantics, storage
//! schema, and TUI rendering belong in other crates.

pub mod analytics;
pub mod ids;

pub use analytics::{
    action_finished_event, listen_qualified_event, now_ms, playback_completed_event,
    playback_paused_event, playback_resumed_event, playback_skipped_event, playback_started_event,
    qualify_listen, redact_spotify_path, search_performed_event, spotify_api_finished_event,
    AnalyticsEvent, AnalyticsEventKind, AnalyticsSink, AnalyticsSource, BackendLabel, HabitBucket,
    HabitWindow, ListenFact, PlaybackSource, Qualification, SkipReason, StoredAnalyticsEvent,
    QUALIFICATION_RULE_VERSION,
};
pub use ids::{AlbumId, ArtistId, PlaylistId, TrackId};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Playback {
    pub item: Option<MediaItem>,
    pub device: Option<Device>,
    pub is_playing: bool,
    pub progress_ms: u64,
    pub shuffle: bool,
    pub repeat: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Queue {
    pub currently_playing: Option<MediaItem>,
    pub items: Vec<MediaItem>,
}

/// Which player implementation the daemon should use to register a
/// Spotify Connect device and stream audio. Domain enum so configuration
/// (in `spotuify-spotify`) and the trait (in `spotuify-player`) can both
/// reference it without a dependency cycle.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BackendKind {
    /// In-process librespot Player + Spirc. Single binary, gapless,
    /// mercury bus available. Will become the default once Phase 9.5
    /// lands stable audio backends across all targets.
    Embedded,
    /// Supervised spotifyd sibling process. Today's default — preserves
    /// existing user behaviour during the Phase 9 rollout.
    #[default]
    Spotifyd,
    /// No local device; remote-control existing Connect devices via the
    /// Web API. Useful for headless servers and Free accounts that can
    /// still browse and steer playback on another device.
    Connect,
}

impl BackendKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Embedded => "embedded",
            Self::Spotifyd => "spotifyd",
            Self::Connect => "connect",
        }
    }

    /// Parse the user-facing string form used in config.toml and the
    /// `--backend` CLI flag. Returns the typo verbatim in the error so
    /// users can see what they typed.
    pub fn parse(value: &str) -> Result<Self, BackendKindParseError> {
        match value {
            "embedded" => Ok(Self::Embedded),
            "spotifyd" => Ok(Self::Spotifyd),
            "connect" => Ok(Self::Connect),
            other => Err(BackendKindParseError {
                value: other.to_string(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendKindParseError {
    pub value: String,
}

impl std::fmt::Display for BackendKindParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "unknown player backend `{}`; expected one of: embedded, spotifyd, connect",
            self.value
        )
    }
}

impl std::error::Error for BackendKindParseError {}

#[cfg(test)]
mod backend_kind_tests {
    use super::BackendKind;

    #[test]
    fn label_is_lowercase_kebab() {
        assert_eq!(BackendKind::Embedded.label(), "embedded");
        assert_eq!(BackendKind::Spotifyd.label(), "spotifyd");
        assert_eq!(BackendKind::Connect.label(), "connect");
    }

    #[test]
    fn parse_round_trips_through_label() {
        for kind in [
            BackendKind::Embedded,
            BackendKind::Spotifyd,
            BackendKind::Connect,
        ] {
            let parsed = BackendKind::parse(kind.label()).unwrap();
            assert_eq!(parsed, kind);
        }
    }

    #[test]
    fn parse_typo_echoes_value_in_error() {
        // Adversarial: error must echo `embeded` so users can fix the
        // exact line they typed. A generic "invalid" would fail this.
        let err = BackendKind::parse("embeded").unwrap_err();
        assert!(err.value.contains("embeded"));
        assert!(err.to_string().contains("embeded"));
    }

    #[test]
    fn default_is_spotifyd_during_phase_9_rollout() {
        // Adversarial: default flip from spotifyd → embedded happens in
        // sub-phase 9.5. Asserting the default lock here ensures no
        // surprise behaviour change for existing users.
        assert_eq!(BackendKind::default(), BackendKind::Spotifyd);
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    Track,
    Episode,
    Album,
    Artist,
    Playlist,
}

impl MediaKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Track => "track",
            Self::Episode => "episode",
            Self::Album => "album",
            Self::Artist => "artist",
            Self::Playlist => "playlist",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MediaItem {
    pub id: Option<String>,
    pub uri: String,
    pub name: String,
    pub subtitle: String,
    pub context: String,
    pub duration_ms: u64,
    pub image_url: Option<String>,
    pub kind: MediaKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub freshness: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explicit: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_playable: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Device {
    pub id: Option<String>,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub is_active: bool,
    pub is_restricted: bool,
    pub volume_percent: Option<u8>,
    pub supports_volume: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub owner: String,
    pub tracks_total: u64,
    pub image_url: Option<String>,
    /// Spotify's playlist-version token (Phase 6.4 schema, Phase 6.5
    /// sync gate). When equal to the local copy, the daemon skips the
    /// expensive `/playlists/{id}/tracks` refetch. Optional because
    /// older cached rows + non-Spotify-sourced playlists may not have
    /// one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_kind_round_trips_through_json_lowercase() {
        let kinds = [
            MediaKind::Track,
            MediaKind::Episode,
            MediaKind::Album,
            MediaKind::Artist,
            MediaKind::Playlist,
        ];
        for kind in kinds {
            let encoded = serde_json::to_string(&kind).unwrap();
            let decoded: MediaKind = serde_json::from_str(&encoded).unwrap();
            assert_eq!(kind, decoded);
            assert_eq!(encoded.trim_matches('"'), kind.label());
        }
    }

    #[test]
    fn media_item_omits_optional_fields_when_none() {
        let item = MediaItem {
            id: None,
            uri: "spotify:track:abc".to_string(),
            name: "Song".to_string(),
            subtitle: String::new(),
            context: String::new(),
            duration_ms: 1000,
            image_url: None,
            kind: MediaKind::Track,
            source: None,
            freshness: None,
            explicit: None,
            is_playable: None,
        };
        let json = serde_json::to_value(&item).unwrap();
        let obj = json.as_object().unwrap();
        assert!(!obj.contains_key("source"));
        assert!(!obj.contains_key("freshness"));
        assert!(!obj.contains_key("explicit"));
        assert!(!obj.contains_key("is_playable"));
    }

    #[test]
    fn playback_default_is_paused_empty() {
        let p = Playback::default();
        assert!(p.item.is_none());
        assert!(p.device.is_none());
        assert!(!p.is_playing);
        assert_eq!(p.progress_ms, 0);
    }

    #[test]
    fn device_renames_kind_to_type_in_json() {
        let device = Device {
            id: Some("dev1".to_string()),
            name: "Phone".to_string(),
            kind: "smartphone".to_string(),
            is_active: false,
            is_restricted: false,
            volume_percent: Some(50),
            supports_volume: true,
        };
        let json = serde_json::to_value(&device).unwrap();
        assert_eq!(
            json.get("type").and_then(|v| v.as_str()),
            Some("smartphone")
        );
        assert!(json.get("kind").is_none());
    }
}

#[cfg(test)]
mod dev_dependencies_imports {
    // Required because serde_json is a dev-dependency of this crate but not a
    // direct dependency. The test module uses it via `serde_json::*` paths.
    #[allow(unused_imports)]
    use serde_json as _;
}
