//! Typed identifier newtypes for Spotify URIs.
//!
//! Display names drift; URIs are stable. Pass `TrackId`, `ArtistId`,
//! `AlbumId`, `PlaylistId` through the workspace instead of `String`
//! so the compiler catches kind mix-ups.
//!
//! `from_uri` accepts a full `spotify:<kind>:<id>` URI and returns
//! `Some(_)` only when the kind matches. The wrapped value is the
//! bare ID (everything after the final colon).

use serde::{Deserialize, Serialize};

macro_rules! spotify_id_newtype {
    ($name:ident, $kind:literal) => {
        #[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Wrap a bare Spotify ID (the part after `spotify:<kind>:`).
            /// Trusts the caller; use [`Self::from_uri`] to parse a
            /// full URI safely.
            pub fn new(id: impl Into<String>) -> Self {
                Self(id.into())
            }

            /// Parse `spotify:<kind>:<id>`. Returns `None` if the
            /// kind does not match or the URI is malformed.
            pub fn from_uri(uri: &str) -> Option<Self> {
                let mut parts = uri.split(':');
                if parts.next()? != "spotify" {
                    return None;
                }
                if parts.next()? != $kind {
                    return None;
                }
                let id = parts.next()?;
                if id.is_empty() || parts.next().is_some() {
                    return None;
                }
                Some(Self(id.to_string()))
            }

            /// Bare ID, no scheme prefix. Use [`Self::to_uri`] to
            /// recover the full `spotify:` URI.
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// Full `spotify:<kind>:<id>` URI.
            pub fn to_uri(&self) -> String {
                format!("spotify:{}:{}", $kind, self.0)
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

spotify_id_newtype!(TrackId, "track");
spotify_id_newtype!(ArtistId, "artist");
spotify_id_newtype!(AlbumId, "album");
spotify_id_newtype!(PlaylistId, "playlist");
