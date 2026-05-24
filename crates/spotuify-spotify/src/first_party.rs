//! First-party (librespot keymaster) Web API credentials.
//!
//! The dev-app PKCE flow (`auth::StoredToken`) is being replaced by
//! librespot's first-party OAuth (keymaster client id) + `login5`. A
//! first-party login is never in Spotify's Development Mode, so it can
//! write playlists where a dev-app token gets a 403.
//!
//! What we persist: only the long-lived librespot-oauth **refresh
//! token** (plus the scopes granted at login, for diagnostics). The Web
//! API bearer itself is always minted live via `login5().auth_token()`
//! and never written to disk. Reusable native playback credentials live
//! in librespot's own cache.
//!
//! Legacy dev-app `StoredToken`s already on disk are detected on load
//! (they carry no `auth_kind` discriminator) and classified as "needs
//! re-login" so the daemon can surface the existing AuthRequired banner.
//!
//! This module is pure data + classification; the librespot calls that
//! produce a `FirstPartyCredentials` live in `spotuify-player` (behind
//! the `embedded-playback` feature), which owns the librespot deps.

use serde::{Deserialize, Serialize};

use crate::auth::StoredToken;

/// Discriminator written into every first-party credential blob. Legacy
/// dev-app `StoredToken`s have no such field, which is how
/// [`classify_credential`] tells the two apart.
pub const FIRST_PARTY_KIND: &str = "first-party";

/// Persisted first-party credential. Only the refresh token is
/// long-lived; the Web API bearer is minted on demand and never stored.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct FirstPartyCredentials {
    /// Marks the blob as first-party. Always [`FIRST_PARTY_KIND`].
    pub auth_kind: String,
    /// librespot-oauth refresh token (keymaster client). Re-bootstraps
    /// the librespot session without a browser when the cached native
    /// credentials are missing.
    pub refresh_token: String,
    /// Scopes granted at login. Informational only — `login5` mints a
    /// full-scope bearer regardless of what was requested here.
    #[serde(default)]
    pub scopes: Vec<String>,
}

impl FirstPartyCredentials {
    pub fn new(refresh_token: impl Into<String>, scopes: Vec<String>) -> Self {
        Self {
            auth_kind: FIRST_PARTY_KIND.to_string(),
            refresh_token: refresh_token.into(),
            scopes,
        }
    }

    /// True when the discriminator marks this as a genuine first-party
    /// blob (guards against a dev-app token deserializing into this
    /// shape, since both carry a `refresh_token`).
    pub fn is_first_party(&self) -> bool {
        self.auth_kind == FIRST_PARTY_KIND
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

/// A stored credential blob, classified by shape.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoredCredential {
    /// First-party keymaster credential — the supported path.
    FirstParty(FirstPartyCredentials),
    /// Legacy dev-app PKCE token. Still readable, but the user must
    /// re-login to switch to the first-party flow.
    LegacyDevApp(StoredToken),
}

/// Classify a raw credential blob read from the keychain or disk cache.
///
/// First-party blobs carry `auth_kind: "first-party"`; legacy dev-app
/// blobs are bare [`StoredToken`]s with no such field. We try
/// first-party first (gated on the discriminator so a dev-app token
/// can't masquerade as one), then fall back to the legacy shape.
/// Returns `None` when neither shape parses.
pub fn classify_credential(raw: &str) -> Option<StoredCredential> {
    if let Ok(creds) = serde_json::from_str::<FirstPartyCredentials>(raw) {
        if creds.is_first_party() && !creds.refresh_token.is_empty() {
            return Some(StoredCredential::FirstParty(creds));
        }
    }
    if let Ok(legacy) = serde_json::from_str::<StoredToken>(raw) {
        return Some(StoredCredential::LegacyDevApp(legacy));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{classify_credential, FirstPartyCredentials, StoredCredential, FIRST_PARTY_KIND};
    use crate::auth::StoredToken;

    fn legacy_token_json() -> String {
        serde_json::to_string(&StoredToken {
            access_token: "dev-access".to_string(),
            refresh_token: "dev-refresh".to_string(),
            expires_at: 1_000,
            scope: "user-read-private".to_string(),
            token_type: "Bearer".to_string(),
        })
        .expect("legacy token serializes")
    }

    #[test]
    fn first_party_round_trips_through_json() {
        let creds =
            FirstPartyCredentials::new("rt-123", vec!["playlist-modify-private".to_string()]);
        let json = creds.to_json().expect("serialize");
        let back: FirstPartyCredentials = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, creds);
        assert_eq!(back.auth_kind, FIRST_PARTY_KIND);
    }

    #[test]
    fn classify_recognizes_first_party_blob() {
        let json = FirstPartyCredentials::new("rt-abc", vec![])
            .to_json()
            .expect("serialize");
        match classify_credential(&json) {
            Some(StoredCredential::FirstParty(creds)) => {
                assert_eq!(creds.refresh_token, "rt-abc");
            }
            other => panic!("expected first-party, got {other:?}"),
        }
    }

    #[test]
    fn classify_recognizes_legacy_dev_app_token() {
        // Adversarial: a dev-app StoredToken also has a `refresh_token`,
        // so without the discriminator guard it could be misread as
        // first-party. It must classify as legacy.
        match classify_credential(&legacy_token_json()) {
            Some(StoredCredential::LegacyDevApp(token)) => {
                assert_eq!(token.access_token, "dev-access");
            }
            other => panic!("expected legacy dev-app, got {other:?}"),
        }
    }

    #[test]
    fn first_party_blob_without_discriminator_is_not_first_party() {
        // A blob with a refresh_token but auth_kind != "first-party"
        // must not be accepted as first-party.
        let raw = r#"{"auth_kind":"","refresh_token":"rt","scopes":[]}"#;
        // It has no access_token/expires_at, so it isn't a valid legacy
        // token either — neither shape, so None.
        assert_eq!(classify_credential(raw), None);
    }

    #[test]
    fn first_party_with_empty_refresh_token_is_rejected() {
        let raw = r#"{"auth_kind":"first-party","refresh_token":"","scopes":[]}"#;
        assert_eq!(classify_credential(raw), None);
    }

    #[test]
    fn garbage_blob_classifies_as_none() {
        assert_eq!(classify_credential("not json"), None);
        assert_eq!(classify_credential("{}"), None);
    }
}
