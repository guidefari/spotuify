//! Phase 6.3: rate-limit middleware for the Spotify Web API client.
//!
//! Three pieces:
//!
//! 1. [`RetryAction`] / [`decide_retry`] — pure function deciding what to
//!    do on a response (retry after delay, give up with typed error).
//!    No I/O; trivially unit-tested.
//!
//! 2. [`BackoffState`] — per-scope token bucket with a `next_eligible_at`
//!    timestamp. Persistable to disk so backoff survives daemon restart
//!    (mxr's `sync_runtime_status.backoff_until` pattern).
//!
//! 3. [`RateLimitedClient`] — thin reqwest wrapper that combines the two
//!    plus a [`Priority`] semaphore. PlaybackControl bypasses the cap;
//!    Foreground and BackgroundSync share it.
//!
//! Pattern adopted from mxr `crates/daemon/src/loops.rs:435-441`
//! (provider-suggested backoff + 10s buffer; exponential [30s, 300s]
//! for other errors) and mxr `crates/provider-gmail/src/client.rs:102-120`
//! (Retry-After parsing).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::error::{classify_response, parse_retry_after, SpotifyError};

/// Priority lane. PlaybackControl bypasses the concurrency cap entirely
/// so user-issued transport commands aren't queued behind background sync.
/// Foreground (user-issued mutations) and BackgroundSync share the cap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Priority {
    PlaybackControl,
    Foreground,
    BackgroundSync,
}

/// Maximum retry attempts on transient errors (5xx, single-shot Network).
/// 429 retries are governed by the Retry-After header and reset the
/// attempt counter so a client behind a sustained rate-limit doesn't
/// give up after 3 tries.
pub const MAX_TRANSIENT_RETRIES: u32 = 3;

/// Exponential backoff base. Each attempt multiplies by 2 with ±25%
/// jitter applied multiplicatively.
pub const BACKOFF_BASE_MS: u64 = 250;

/// Upper bound on a single transient backoff. Beyond this, give up and
/// surface the error so the daemon's higher-level scheduler decides
/// whether to retry later.
pub const BACKOFF_CEILING_MS: u64 = 30_000;

/// Action to take after seeing a response or a transport-level failure.
#[derive(Debug)]
pub enum RetryAction {
    /// Sleep then retry the same request.
    Retry { delay: Duration },
    /// Surface this typed error to the caller. No further retries.
    GiveUp(SpotifyError),
    /// The response was 2xx. Caller should consume it.
    Success,
}

impl PartialEq for RetryAction {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Success, Self::Success) => true,
            (Self::Retry { delay: a }, Self::Retry { delay: b }) => a == b,
            // GiveUp comparisons aren't useful in tests — we match on the
            // SpotifyError variant directly. Treat unequal.
            _ => false,
        }
    }
}

/// Decide what to do after seeing a response.
///
/// `attempt` is 0-indexed (0 = first attempt, just made; 1 = first retry,
/// considering whether to make second; …).
///
/// Pure function: no clocks, no I/O. Take `now` so tests are deterministic.
pub fn decide_retry(
    attempt: u32,
    status: u16,
    retry_after: Option<&str>,
    endpoint: &str,
    body: &str,
    now: DateTime<Utc>,
    rng: &mut impl Rng,
) -> RetryAction {
    if (200..300).contains(&status) || status == 304 {
        return RetryAction::Success;
    }

    // 429 -> always honour Retry-After (or default 60s). Doesn't count
    // against transient-retry budget; sustained rate-limits will loop
    // until the daemon decides to escalate (caller's responsibility).
    if status == 429 {
        let delay = parse_retry_after(retry_after, now);
        return RetryAction::Retry { delay };
    }

    // 401 -> the auth layer above us will refresh and retry. Surface the
    // typed AuthExpired and let it handle the token swap.
    if status == 401 {
        return RetryAction::GiveUp(SpotifyError::AuthExpired);
    }

    // 5xx -> retry with jittered exponential backoff up to MAX_TRANSIENT_RETRIES.
    if (500..600).contains(&status) {
        if attempt + 1 >= MAX_TRANSIENT_RETRIES {
            return RetryAction::GiveUp(classify_response(
                status, retry_after, endpoint, body, now,
            ));
        }
        let delay = jittered_backoff(attempt, rng);
        return RetryAction::Retry { delay };
    }

    // Everything else (4xx, weird statuses) -> classify and give up.
    RetryAction::GiveUp(classify_response(status, retry_after, endpoint, body, now))
}

/// Compute the jittered exponential backoff for attempt `n` (0-indexed).
///
/// `n=0` ≈ 250ms ± 25%
/// `n=1` ≈ 500ms ± 25%
/// `n=2` ≈ 1000ms ± 25%
/// …
///
/// Capped at `BACKOFF_CEILING_MS`.
pub fn jittered_backoff(attempt: u32, rng: &mut impl Rng) -> Duration {
    let base = BACKOFF_BASE_MS.saturating_mul(1u64 << attempt.min(10));
    let base = base.min(BACKOFF_CEILING_MS);
    let jitter: f64 = rng.gen_range(0.75..=1.25);
    let final_ms = (base as f64 * jitter).round() as u64;
    Duration::from_millis(final_ms.clamp(1, BACKOFF_CEILING_MS))
}

/// Per-scope backoff state. `next_eligible_at_ms` is the unix epoch
/// timestamp before which no request to this scope should be issued.
///
/// Persistable to disk so daemon restart respects an active backoff
/// (otherwise a crash-and-restart loop hammers Spotify and trips
/// auth-revoked).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScopeState {
    pub next_eligible_at_ms: Option<i64>,
    pub last_rate_limited_at_ms: Option<i64>,
}

/// All scope budgets in one map. Serialised to JSON at the path passed
/// to [`RateLimitedClient::new`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BackoffState {
    pub scopes: HashMap<String, ScopeState>,
}

impl BackoffState {
    /// Time in millis to wait before this scope's next request is
    /// eligible. `0` means immediate.
    pub fn wait_ms(&self, scope: &str, now_ms: i64) -> i64 {
        self.scopes
            .get(scope)
            .and_then(|s| s.next_eligible_at_ms)
            .map(|target| (target - now_ms).max(0))
            .unwrap_or(0)
    }

    pub fn record_rate_limit(&mut self, scope: &str, now_ms: i64, retry_after: Duration) {
        let s = self.scopes.entry(scope.to_string()).or_default();
        s.last_rate_limited_at_ms = Some(now_ms);
        s.next_eligible_at_ms = Some(now_ms + retry_after.as_millis() as i64);
    }

    pub fn clear(&mut self, scope: &str) {
        if let Some(s) = self.scopes.get_mut(scope) {
            s.next_eligible_at_ms = None;
        }
    }

    pub fn load(path: &PathBuf) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &PathBuf) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let raw = serde_json::to_string(self).map_err(std::io::Error::other)?;
        std::fs::write(path, raw)
    }
}

/// Thin reqwest wrapper that applies the rate-limit policy. Full
/// integration with the daemon Spotify client is wired in a follow-up
/// (the existing `src/spotify.rs` retains its string-bail flow until
/// migrated; new code paths use this client).
pub struct RateLimitedClient {
    pub(crate) inner: reqwest::Client,
    pub(crate) backoff: Arc<RwLock<BackoffState>>,
    pub(crate) bucket_path: Option<PathBuf>,
    pub(crate) foreground_sem: Arc<tokio::sync::Semaphore>,
    pub(crate) background_sem: Arc<tokio::sync::Semaphore>,
}

impl RateLimitedClient {
    /// Build a new client. Persist the bucket state to `bucket_path` if
    /// provided so backoff survives daemon restart.
    ///
    /// `foreground_permits` and `background_permits` control the
    /// concurrency caps for each lane. PlaybackControl is unbounded.
    pub fn new(
        inner: reqwest::Client,
        bucket_path: Option<PathBuf>,
        foreground_permits: usize,
        background_permits: usize,
    ) -> Self {
        let backoff = match &bucket_path {
            Some(path) => BackoffState::load(path),
            None => BackoffState::default(),
        };
        Self {
            inner,
            backoff: Arc::new(RwLock::new(backoff)),
            bucket_path,
            foreground_sem: Arc::new(tokio::sync::Semaphore::new(foreground_permits)),
            background_sem: Arc::new(tokio::sync::Semaphore::new(background_permits)),
        }
    }

    /// Snapshot of the persistent state, useful for tests + diagnostics.
    pub fn backoff_snapshot(&self) -> BackoffState {
        self.backoff.read().clone()
    }

    /// Reset all scopes. Used by `spotuify cache reset --confirm`.
    pub fn reset(&self) {
        self.backoff.write().scopes.clear();
        if let Some(path) = &self.bucket_path {
            let _ = std::fs::remove_file(path);
        }
    }

    pub fn inner(&self) -> &reqwest::Client {
        &self.inner
    }
}
