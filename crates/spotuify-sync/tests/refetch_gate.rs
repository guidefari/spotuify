//! Phase 6.5 — sync refetch gate decision tests.

use spotuify_protocol::{DaemonEvent, SyncTargetData};
use spotuify_spotify::SpotifyClient;
use spotuify_store::Store;
use spotuify_sync::{
    should_refetch_playlist_tracks, should_refetch_saved_tracks, sync_target, SyncContext,
};
use tokio::sync::watch;

// --- Playlist snapshot-id gate ---

#[test]
fn first_sync_with_no_local_snapshot_refetches() {
    assert!(should_refetch_playlist_tracks(None, Some("snap-1")));
}

#[test]
fn matching_snapshots_skip_refetch() {
    assert!(!should_refetch_playlist_tracks(
        Some("snap-1"),
        Some("snap-1")
    ));
}

#[test]
fn differing_snapshots_trigger_refetch() {
    assert!(should_refetch_playlist_tracks(
        Some("snap-1"),
        Some("snap-2")
    ));
}

#[test]
fn missing_remote_snapshot_refetches_defensively() {
    // The Spotify response didn't include snapshot_id -- we can't
    // prove unchanged, so refetch.
    assert!(should_refetch_playlist_tracks(Some("snap-1"), None));
}

#[test]
fn both_missing_snapshots_refetches() {
    // Cold start with a playlist that never carries snapshot_id.
    assert!(should_refetch_playlist_tracks(None, None));
}

#[test]
fn empty_string_snapshot_is_distinct_from_missing() {
    // Implementation detail: empty string is a valid (if degenerate)
    // snapshot id; it shouldn't be treated as None.
    assert!(!should_refetch_playlist_tracks(Some(""), Some("")));
    assert!(should_refetch_playlist_tracks(Some(""), Some("real-snap")));
}

// --- Saved-tracks page-0 unchanged shortcut ---

#[test]
fn matching_total_and_first_ids_skips_refetch() {
    let local = ["track:1", "track:2", "track:3"];
    let remote = ["track:1", "track:2", "track:3"];
    assert!(!should_refetch_saved_tracks(100, &local, 100, &remote));
}

#[test]
fn differing_total_triggers_refetch() {
    let local = ["track:1", "track:2"];
    let remote = ["track:1", "track:2"];
    // total changed even though the visible page matches -- maybe a
    // delete at the bottom. Refetch to be safe.
    assert!(should_refetch_saved_tracks(100, &local, 99, &remote));
}

#[test]
fn new_track_at_top_changes_first_ids_and_refetches() {
    let local = ["old-1", "old-2"];
    let remote = ["new-1", "old-1", "old-2"];
    assert!(should_refetch_saved_tracks(100, &local, 101, &remote));
}

#[test]
fn same_total_but_different_first_ids_refetches() {
    // Rare reorder + replace where total stays equal. Refetch.
    let local = ["a", "b", "c"];
    let remote = ["b", "a", "c"];
    assert!(should_refetch_saved_tracks(50, &local, 50, &remote));
}

#[test]
fn empty_library_matches_empty_library() {
    let empty: [&str; 0] = [];
    assert!(!should_refetch_saved_tracks(0, &empty, 0, &empty));
}

#[test]
fn zero_local_versus_nonzero_remote_refetches() {
    let empty: [&str; 0] = [];
    let remote = ["track:1"];
    assert!(should_refetch_saved_tracks(0, &empty, 1, &remote));
}

struct FakeCtx {
    store: Store,
    shutdown_rx: watch::Receiver<bool>,
}

#[async_trait::async_trait]
impl SyncContext for FakeCtx {
    fn shutdown_receiver(&self) -> watch::Receiver<bool> {
        self.shutdown_rx.clone()
    }

    fn store(&self) -> &Store {
        &self.store
    }

    fn emit_event(&self, _event: DaemonEvent) {}

    async fn spotify_client(&self) -> anyhow::Result<SpotifyClient> {
        Ok(SpotifyClient::fake()?)
    }
}

#[tokio::test]
async fn queue_sync_persists_current_and_upcoming_items() {
    let (_shutdown_tx, shutdown_rx) = watch::channel(false);
    let ctx = FakeCtx {
        store: Store::in_memory().await.expect("in-memory store"),
        shutdown_rx,
    };

    let summary = sync_target(&ctx, SyncTargetData::Queue)
        .await
        .expect("queue sync");
    let queue = ctx
        .store
        .latest_queue(10)
        .await
        .expect("queue cache read")
        .expect("queue cache should exist");

    assert_eq!(summary.queue_snapshots, 1);
    assert_eq!(summary.queue_items, 1);
    assert!(queue.currently_playing.is_some());
    assert_eq!(queue.items.len(), 1);
}

#[tokio::test]
async fn playlist_sync_fetches_tracks_on_cold_start_then_skips_when_snapshot_matches() {
    let (_shutdown_tx, shutdown_rx) = watch::channel(false);
    let ctx = FakeCtx {
        store: Store::in_memory().await.expect("in-memory store"),
        shutdown_rx,
    };

    let first = sync_target(&ctx, SyncTargetData::Playlists)
        .await
        .expect("first playlist sync");
    assert_eq!(first.playlists, 1);
    assert_eq!(
        first.playlist_items, 2,
        "cold start must fetch playlist tracks before persisting remote snapshot"
    );

    let second = sync_target(&ctx, SyncTargetData::Playlists)
        .await
        .expect("second playlist sync");
    assert_eq!(second.playlists, 1);
    assert_eq!(
        second.playlist_items, 0,
        "matching snapshot should skip expensive tracks refetch"
    );
}

#[tokio::test]
async fn library_sync_skips_saved_tracks_when_page_zero_is_unchanged() {
    let (_shutdown_tx, shutdown_rx) = watch::channel(false);
    let ctx = FakeCtx {
        store: Store::in_memory().await.expect("in-memory store"),
        shutdown_rx,
    };

    let first = sync_target(&ctx, SyncTargetData::Library)
        .await
        .expect("first library sync");
    assert_eq!(first.library_items, 3);

    let second = sync_target(&ctx, SyncTargetData::Library)
        .await
        .expect("second library sync");
    assert_eq!(
        second.library_items, 1,
        "matching saved-track page 0 should skip the full saved-track refetch and only refresh albums"
    );
}
