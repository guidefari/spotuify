//! Re-export bridge for spotuify-spotify::client.
//!
//! The SpotifyClient implementation lives in
//! `crates/spotuify-spotify/src/client.rs`. Existing binary call
//! sites (`crate::spotify::SpotifyClient`, `crate::spotify::Playlist`,
//! etc.) keep compiling through this shim. Future PRs migrate
//! callers to `use spotuify_spotify::...` directly and remove this
//! file.

pub use spotuify_spotify::client::*;
