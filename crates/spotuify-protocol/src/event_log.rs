//! Phase 6.9 — recent-event ring buffer + doctor finding derivation.
//!
//! The daemon keeps a small in-memory log of recent `DaemonEvent`s
//! (RateLimited, AuthError, SchemaCompat). The `findings_from`
//! function takes a snapshot of that log and returns the
//! [`DoctorFinding`]s for the doctor report.
//!
//! Pure function — testable without spinning up the daemon.

use crate::{
    DaemonEvent, DoctorFinding, DoctorFindingCategory, DoctorFindingSeverity,
};

/// One event remembered in the daemon's ring buffer. We don't store the
/// full `DaemonEvent` (which can be large for SyncFinished etc.) — only
/// the variants that drive doctor findings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoggedEvent {
    pub at_ms: i64,
    pub kind: LoggedKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoggedKind {
    RateLimited {
        retry_after_secs: u64,
        scope: String,
    },
    AuthError {
        kind_str: String,
    },
    SchemaCompat {
        endpoint: String,
        missing_keys: Vec<String>,
    },
}

impl LoggedEvent {
    /// Lift the subset of `DaemonEvent`s we track. Other variants
    /// return `None` and are ignored by the buffer.
    pub fn from(event: &DaemonEvent, at_ms: i64) -> Option<Self> {
        let kind = match event {
            DaemonEvent::RateLimited {
                retry_after_secs,
                scope,
            } => LoggedKind::RateLimited {
                retry_after_secs: *retry_after_secs,
                scope: scope.clone(),
            },
            DaemonEvent::AuthError { kind } => LoggedKind::AuthError {
                kind_str: format!("{:?}", kind),
            },
            DaemonEvent::SchemaCompat {
                endpoint,
                missing_keys,
            } => LoggedKind::SchemaCompat {
                endpoint: endpoint.clone(),
                missing_keys: missing_keys.clone(),
            },
            _ => return None,
        };
        Some(LoggedEvent { at_ms, kind })
    }
}

/// Build the doctor findings from a recent-event snapshot.
///
/// Findings are emitted when an event happened within its lookback
/// window:
/// - RateLimited within the last 5 minutes → Warning
/// - AuthError ever (at least one entry) → Error
/// - SchemaCompat within the last hour → Info
///
/// `now_ms` is provided rather than read from `SystemTime` so tests
/// are deterministic.
pub fn findings_from(events: &[LoggedEvent], now_ms: i64) -> Vec<DoctorFinding> {
    const RATE_LIMIT_LOOKBACK_MS: i64 = 5 * 60 * 1000;
    const SCHEMA_COMPAT_LOOKBACK_MS: i64 = 60 * 60 * 1000;

    let mut findings = Vec::new();

    if let Some(latest) = events.iter().rev().find(|e| {
        matches!(e.kind, LoggedKind::RateLimited { .. })
            && now_ms - e.at_ms <= RATE_LIMIT_LOOKBACK_MS
    }) {
        if let LoggedKind::RateLimited { retry_after_secs, scope } = &latest.kind {
            findings.push(DoctorFinding {
                category: DoctorFindingCategory::Network,
                severity: DoctorFindingSeverity::Warning,
                message: format!(
                    "Rate limited on `{scope}` — backing off for {retry_after_secs}s."
                ),
                remediation: vec![
                    "Wait for the backoff window before retrying.".to_string(),
                    "Reduce sync frequency in config.toml if this recurs.".to_string(),
                ],
            });
        }
    }

    if events
        .iter()
        .rev()
        .any(|e| matches!(e.kind, LoggedKind::AuthError { .. }))
    {
        findings.push(DoctorFinding {
            category: DoctorFindingCategory::Auth,
            severity: DoctorFindingSeverity::Error,
            message:
                "Authentication failed since last clean run. Re-run `spotuify login` to refresh credentials."
                    .to_string(),
            remediation: vec!["spotuify login".to_string()],
        });
    }

    let recent_compat: Vec<&LoggedEvent> = events
        .iter()
        .rev()
        .filter(|e| {
            matches!(e.kind, LoggedKind::SchemaCompat { .. })
                && now_ms - e.at_ms <= SCHEMA_COMPAT_LOOKBACK_MS
        })
        .collect();
    if !recent_compat.is_empty() {
        let endpoints: Vec<String> = recent_compat
            .iter()
            .filter_map(|e| match &e.kind {
                LoggedKind::SchemaCompat { endpoint, .. } => Some(endpoint.clone()),
                _ => None,
            })
            .collect();
        let endpoints_str = endpoints.join(", ");
        findings.push(DoctorFinding {
            category: DoctorFindingCategory::Network,
            severity: DoctorFindingSeverity::Info,
            message: format!(
                "Spotify changed response shapes for {endpoints_str}; compat layer applied."
            ),
            remediation: vec![],
        });
    }

    findings
}

/// Simple bounded FIFO. Append `push` adds; oldest entry drops when
/// the buffer exceeds `cap`. Used by the daemon's event tap.
#[derive(Debug, Clone)]
pub struct EventLog {
    cap: usize,
    items: Vec<LoggedEvent>,
}

impl EventLog {
    pub fn new(cap: usize) -> Self {
        Self {
            cap,
            items: Vec::with_capacity(cap),
        }
    }

    pub fn push(&mut self, event: LoggedEvent) {
        if self.items.len() >= self.cap {
            self.items.remove(0);
        }
        self.items.push(event);
    }

    pub fn snapshot(&self) -> Vec<LoggedEvent> {
        self.items.clone()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}
