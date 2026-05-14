//! Phase 10 — listen qualification rule tests.
//!
//! Blueprint rule (definitive, per `docs/blueprint/16-analytics.md`):
//! a listen qualifies when `duration_ms > 30_000` AND
//! `audible_ms >= max(30_000, min(duration_ms / 2, 240_000))`.
//!
//! Adversarial coverage:
//! - The 30s minimum duration floor (29.999s never qualifies).
//! - The 30s minimum audible floor (15s audible on a 30.001s track must
//!   NOT qualify; 30s audible must).
//! - The 4-minute cap on the percentage formula (10-min track qualifies
//!   at exactly 240s audible, not 300s).
//! - Rule version is stamped so historic facts stay computable under
//!   future tweaks.
//! - The result struct exposes the threshold so callers can render
//!   "X of Y minutes" progress bars.

use spotuify_core::{qualify_listen, QUALIFICATION_RULE_VERSION};

#[test]
fn track_under_30s_never_qualifies() {
    for audible in [0_i64, 10_000, 25_000, 29_999, 30_000, 60_000] {
        let q = qualify_listen(29_999, audible);
        assert!(
            !q.qualified,
            "29.999s track must never qualify (tried audible={audible}ms)"
        );
    }
}

#[test]
fn track_at_exactly_30s_must_be_over_30s_to_qualify() {
    // The rule reads `duration_ms > 30_000` (strict). A track that is
    // exactly 30s does not qualify regardless of audible time.
    let q = qualify_listen(30_000, 60_000);
    assert!(
        !q.qualified,
        "exactly 30_000ms must not qualify (rule is strict >)"
    );
}

#[test]
fn qualifies_when_audible_reaches_50_percent_for_short_tracks() {
    // 60s track -> 50% of 60s = 30s, capped at min(30s, 4min) = 30s.
    // The blueprint floors this at 30s too -> threshold is 30s.
    let q = qualify_listen(60_000, 30_000);
    assert!(q.qualified);
    assert_eq!(q.threshold_ms, 30_000);
}

#[test]
fn short_track_below_audible_floor_does_not_qualify() {
    // 60s track with only 25s audible. 50% would be 30s; floor is 30s.
    // 25s < 30s -> not qualified.
    let q = qualify_listen(60_000, 25_000);
    assert!(!q.qualified);
    assert_eq!(q.threshold_ms, 30_000);
}

#[test]
fn ten_minute_track_qualifies_at_four_minute_cap() {
    // 600s track -> 50% would be 300s, capped at 240s (4min).
    let q = qualify_listen(600_000, 240_000);
    assert!(
        q.qualified,
        "240s audible on a 600s track must qualify (4min cap applies)"
    );
    assert_eq!(q.threshold_ms, 240_000);
}

#[test]
fn ten_minute_track_below_four_minute_cap_does_not_qualify() {
    let q = qualify_listen(600_000, 239_999);
    assert!(!q.qualified);
    assert_eq!(q.threshold_ms, 240_000);
}

#[test]
fn rule_version_is_stamped_at_one() {
    // The rule version must be 1 for the inaugural Phase 10 ship; the
    // value lives in qualification_rules row 1.
    assert_eq!(QUALIFICATION_RULE_VERSION, 1);
    let q = qualify_listen(180_000, 90_000);
    assert_eq!(q.rule_version, 1);
}

#[test]
fn three_minute_track_uses_50_percent_threshold_when_lower_than_cap() {
    // 180s track -> 50% = 90s, less than 240s cap and greater than 30s floor.
    let q = qualify_listen(180_000, 90_000);
    assert!(q.qualified);
    assert_eq!(q.threshold_ms, 90_000);

    let q = qualify_listen(180_000, 89_999);
    assert!(!q.qualified);
    assert_eq!(q.threshold_ms, 90_000);
}

#[test]
fn threshold_is_max_of_30s_and_min_of_half_or_4min() {
    // Boundary cases: walk a few durations and assert the threshold
    // matches max(30s, min(duration/2, 4min)).
    let cases: &[(i64, i64)] = &[
        (40_000, 30_000),   // half = 20s; floor pushes to 30s
        (90_000, 45_000),   // half = 45s
        (200_000, 100_000), // half = 100s
        (480_000, 240_000), // half = 240s, hits cap exactly
        (900_000, 240_000), // half = 450s, clamped to cap
    ];
    for &(duration, expected) in cases {
        let q = qualify_listen(duration, 0);
        assert_eq!(
            q.threshold_ms, expected,
            "duration={duration}ms must yield threshold {expected}ms, got {}",
            q.threshold_ms
        );
    }
}

#[test]
fn negative_or_zero_audible_never_qualifies() {
    let q = qualify_listen(180_000, -1);
    assert!(!q.qualified);
    let q = qualify_listen(180_000, 0);
    assert!(!q.qualified);
}
