//! Phase 6.8 — token refresh scheduling logic.
//!
//! Pure function: given the current time and the token's `expires_at`,
//! decide whether to refresh proactively.
//!
//! Used by the daemon's background scheduler and by the on-demand
//! refresh path in `auth::access_token_cached`. Both call this before
//! touching the network.
//!
//! Rationale (per Phase 6.8 spec): refreshing on 401 is correct but
//! slow — the user sees a multi-second pause on their first action
//! after a long idle. Refreshing at `expires_at - PROACTIVE_HEADROOM`
//! moves that pause into the background sync loop where it's invisible.

use std::time::Duration;

/// Refresh the access token this far before it expires. 60s is the
/// Phase 6.8 default; configurable via daemon settings later.
pub const PROACTIVE_HEADROOM: Duration = Duration::from_secs(60);

/// Decide whether to refresh now.
///
/// `now` and `expires_at` are unix-epoch seconds. Returning `true` means
/// the caller should run a refresh; `false` means the cached token is
/// still good.
///
/// Special cases:
/// - `expires_at == 0` (unset) → refresh (initial bootstrap)
/// - `expires_at < now` (already expired) → refresh
/// - `expires_at - now <= headroom` → proactive refresh
/// - otherwise → no refresh
pub fn should_refresh(now: i64, expires_at: i64, headroom: Duration) -> bool {
    if expires_at == 0 {
        return true;
    }
    if expires_at <= now {
        return true;
    }
    let remaining_secs = expires_at - now;
    remaining_secs <= headroom.as_secs() as i64
}

/// Time until the next proactive refresh fires, given the current
/// `expires_at`. Used by the daemon scheduler to set a sleep duration.
/// Returns `None` if a refresh should happen right now.
pub fn next_refresh_in(now: i64, expires_at: i64, headroom: Duration) -> Option<Duration> {
    if should_refresh(now, expires_at, headroom) {
        return None;
    }
    let target = expires_at - headroom.as_secs() as i64;
    let secs = (target - now).max(0) as u64;
    Some(Duration::from_secs(secs))
}
