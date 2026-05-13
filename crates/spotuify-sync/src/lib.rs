//! Phase 6.5 sync refetch gate helpers + Phase 7 SyncContext trait
//! scaffolding.
//!
//! Pure functions for the daemon's background sync loop to decide
//! whether to skip expensive paginated refetches when nothing has
//! changed since the last successful sync.
//!
//! The [`SyncContext`] trait abstracts over the binary's `DaemonState`
//! so a future extraction of `src/sync.rs` into this crate depends on
//! the trait rather than on the binary type. Once that move happens
//! the binary just `impl SyncContext for DaemonState` and the sync
//! loop swaps `&Arc<DaemonState>` for `&impl SyncContext`.
//!
//! Patterns from ncspot `library.rs:140-148` (snapshot_id-aware
//! playlist sync) and ncspot `library.rs:499-514` (saved-tracks
//! page-0 unchanged shortcut).

use spotuify_protocol::DaemonEvent;
use spotuify_store::Store;
use tokio::sync::watch;

/// Context the sync engine needs from its host process. The binary's
/// `DaemonState` impls this; tests can supply a fake implementation.
///
/// Methods are intentionally minimal -- only what the existing sync
/// loop calls today. Anything new sync needs should be added here
/// first.
#[async_trait::async_trait]
pub trait SyncContext: Send + Sync {
    /// Tokio watch receiver that fires `true` on daemon shutdown.
    fn shutdown_receiver(&self) -> watch::Receiver<bool>;
    /// Persistent cache. Sync reads snapshot_ids and writes
    /// freshness-tagged rows.
    fn store(&self) -> &Store;
    /// Broadcast a daemon event. Sync emits SyncStarted/SyncFinished
    /// (and via the upstream cycle: RateLimited, AuthError,
    /// SchemaCompat).
    fn emit_event(&self, event: DaemonEvent);
}

/// Decide whether to refetch a playlist's full track listing.
///
/// The Spotify Playlist envelope carries `snapshot_id`, a string token
/// that changes on every mutation. Comparing the local cached value
/// against the fresh `GET /playlists/{id}` response tells us whether
/// the expensive paginated `GET /playlists/{id}/tracks` call is worth
/// making.
///
/// Returns true when in doubt -- a missing snapshot on either side
/// means we can't prove unchanged.
pub fn should_refetch_playlist_tracks(
    local_snapshot: Option<&str>,
    remote_snapshot: Option<&str>,
) -> bool {
    match (local_snapshot, remote_snapshot) {
        // First sync: nothing local yet.
        (None, _) => true,
        // Remote didn't include a snapshot id; can't compare.
        (_, None) => true,
        // Both present -- refetch only if they differ.
        (Some(local), Some(remote)) => local != remote,
    }
}

/// Decide whether to refetch the user's saved-tracks library beyond
/// page 0.
///
/// Spotify's saved-tracks endpoint returns `(total, items)` per page.
/// If both the total count AND the first page's IDs match what we
/// have locally, the library is unchanged and we can skip the
/// remaining pages.
///
/// Ordering matters: Spotify returns saved tracks in reverse-added
/// order, so a new add at the top changes both `local_first_ids` and
/// `total`.
///
/// This is an approximation: a rare reorder-without-add-or-remove
/// would slip through. Acceptable trade-off given the API-cost
/// savings for the common steady-state case.
pub fn should_refetch_saved_tracks(
    local_total: u64,
    local_first_ids: &[&str],
    remote_total: u64,
    remote_first_ids: &[&str],
) -> bool {
    if local_total != remote_total {
        return true;
    }
    local_first_ids != remote_first_ids
}
