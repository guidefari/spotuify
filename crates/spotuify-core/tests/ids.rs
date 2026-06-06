#![allow(clippy::panic, clippy::unwrap_used)]

//! Phase 10 — typed ID newtypes.
//!
//! Adversarial coverage:
//! - `from_uri` parses well-formed `spotify:track:XYZ` / `spotify:artist:XYZ`
//!   / `spotify:album:XYZ` / `spotify:playlist:XYZ`.
//! - Wrong kind returns None (an artist URI does not parse as a TrackId).
//! - Truncated, scheme-missing, and empty inputs return None.
//! - `as_str()` round-trips through Serde JSON.
//! - Equality + hash are by string content (so HashSet dedupes by URI).

use spotuify_core::{AlbumId, ArtistId, PlaylistId, TrackId};
use std::collections::HashSet;

#[test]
fn track_id_parses_spotify_uri() {
    let id = TrackId::from_uri("spotify:track:5MhsZlmKJG6X5kTHkdwC4B").unwrap();
    assert_eq!(id.as_str(), "5MhsZlmKJG6X5kTHkdwC4B");
}

#[test]
fn track_id_rejects_other_kinds() {
    assert!(TrackId::from_uri("spotify:artist:5MhsZlmKJG6X5kTHkdwC4B").is_none());
    assert!(TrackId::from_uri("spotify:album:5MhsZlmKJG6X5kTHkdwC4B").is_none());
    assert!(TrackId::from_uri("spotify:playlist:abc").is_none());
}

#[test]
fn artist_id_parses_spotify_uri() {
    let id = ArtistId::from_uri("spotify:artist:1vyhD5VmyZ7KMfW5gqLgo5").unwrap();
    assert_eq!(id.as_str(), "1vyhD5VmyZ7KMfW5gqLgo5");
}

#[test]
fn album_id_parses_spotify_uri() {
    let id = AlbumId::from_uri("spotify:album:5oWFZNFKxYU2drNcG2TbS5").unwrap();
    assert_eq!(id.as_str(), "5oWFZNFKxYU2drNcG2TbS5");
}

#[test]
fn playlist_id_parses_spotify_uri() {
    let id = PlaylistId::from_uri("spotify:playlist:37i9dQZF1DXcBWIGoYBM5M").unwrap();
    assert_eq!(id.as_str(), "37i9dQZF1DXcBWIGoYBM5M");
}

#[test]
fn malformed_uris_return_none() {
    assert!(TrackId::from_uri("").is_none());
    assert!(TrackId::from_uri("spotify:").is_none());
    assert!(TrackId::from_uri("spotify:track:").is_none());
    assert!(TrackId::from_uri("track:5MhsZlmKJG6X5kTHkdwC4B").is_none());
    assert!(TrackId::from_uri("not-a-uri").is_none());
}

#[test]
fn ids_serde_round_trip_as_string() {
    let id = TrackId::from_uri("spotify:track:5MhsZlmKJG6X5kTHkdwC4B").unwrap();
    let json = serde_json::to_string(&id).unwrap();
    assert_eq!(json, "\"5MhsZlmKJG6X5kTHkdwC4B\"");
    let back: TrackId = serde_json::from_str(&json).unwrap();
    assert_eq!(back, id);
}

#[test]
fn hashset_dedupes_by_underlying_id() {
    let a = TrackId::from_uri("spotify:track:5MhsZlmKJG6X5kTHkdwC4B").unwrap();
    let b = TrackId::from_uri("spotify:track:5MhsZlmKJG6X5kTHkdwC4B").unwrap();
    let c = TrackId::from_uri("spotify:track:1vyhD5VmyZ7KMfW5gqLgo5").unwrap();
    let set: HashSet<_> = [a, b, c].into_iter().collect();
    assert_eq!(set.len(), 2);
}
