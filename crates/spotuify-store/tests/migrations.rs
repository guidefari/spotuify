//! Phase 6.4 — schema migration v2 + cache_version gate tests.
//!
//! Adversarial coverage:
//! - v1 → v2 migration is idempotent (running twice is a no-op).
//! - v2 adds the columns Phase 6 needs and they default correctly.
//! - Running against a future-version store (forward-incompat) is
//!   detected and refused rather than silently corrupting data.
//! - check_cache_version() reports the right state for tooling.

use spotuify_store::{Store, CACHE_VERSION};
use sqlx::Row;

async fn fresh_store() -> Store {
    Store::in_memory().await.expect("in_memory store")
}

async fn column_exists(store: &Store, table: &str, column: &str) -> bool {
    let rows = sqlx::query(&format!("PRAGMA table_info({table})"))
        .fetch_all(store.reader())
        .await
        .unwrap();
    rows.iter()
        .any(|row| row.get::<String, _>("name") == column)
}

async fn column_default(store: &Store, table: &str, column: &str) -> Option<String> {
    let rows = sqlx::query(&format!("PRAGMA table_info({table})"))
        .fetch_all(store.reader())
        .await
        .unwrap();
    rows.into_iter()
        .find(|row| row.get::<String, _>("name") == column)
        .and_then(|row| row.try_get::<String, _>("dflt_value").ok())
}

#[tokio::test]
async fn test_cache_version_constant_is_three() {
    // Bumped from 2 -> 3 when migration 003_receipts landed (Phase 6.6).
    assert_eq!(CACHE_VERSION, 3);
}

#[tokio::test]
async fn test_v1_to_v2_migration_is_idempotent() {
    // Building a fresh in-memory store runs both migrations.
    // Running migrations again must not error and must leave row counts
    // unchanged.
    let store = fresh_store().await;
    let before = store.cache_status(0).await.unwrap();
    store
        .run_migrations_idempotent_for_test()
        .await
        .unwrap();
    let after = store.cache_status(0).await.unwrap();
    assert_eq!(before.media_items, after.media_items);
    assert_eq!(before.playlists, after.playlists);
}

#[tokio::test]
async fn test_v2_playlists_has_snapshot_id_column() {
    let store = fresh_store().await;
    assert!(
        column_exists(&store, "playlists", "snapshot_id").await,
        "v2 must add playlists.snapshot_id"
    );
}

#[tokio::test]
async fn test_v2_playlist_items_has_snapshot_id_at_fetch_column() {
    let store = fresh_store().await;
    assert!(
        column_exists(&store, "playlist_items", "snapshot_id_at_fetch").await,
        "v2 must add playlist_items.snapshot_id_at_fetch"
    );
}

#[tokio::test]
async fn test_v2_media_items_has_freshness_class_default_unknown() {
    let store = fresh_store().await;
    assert!(
        column_exists(&store, "media_items", "freshness_class").await,
        "v2 must add media_items.freshness_class"
    );
    let default = column_default(&store, "media_items", "freshness_class").await;
    assert!(
        default.as_deref().map(str::trim).map(|d| d.trim_matches('\''))
            == Some("unknown"),
        "freshness_class must default to 'unknown', got {default:?}"
    );
}

#[tokio::test]
async fn test_v2_media_items_has_sync_generation_default_zero() {
    let store = fresh_store().await;
    assert!(
        column_exists(&store, "media_items", "sync_generation").await,
        "v2 must add media_items.sync_generation"
    );
    let default = column_default(&store, "media_items", "sync_generation").await;
    assert_eq!(default.as_deref(), Some("0"), "sync_generation default should be 0");
}

#[tokio::test]
async fn test_v2_devices_has_freshness_columns() {
    let store = fresh_store().await;
    assert!(column_exists(&store, "devices", "freshness_class").await);
    assert!(column_exists(&store, "devices", "sync_generation").await);
}

#[tokio::test]
async fn test_v2_playback_snapshots_has_freshness_columns() {
    let store = fresh_store().await;
    assert!(column_exists(&store, "playback_snapshots", "freshness_class").await);
    assert!(column_exists(&store, "playback_snapshots", "sync_generation").await);
}

#[tokio::test]
async fn test_v2_recent_items_has_freshness_columns() {
    let store = fresh_store().await;
    assert!(column_exists(&store, "recent_items", "freshness_class").await);
    assert!(column_exists(&store, "recent_items", "sync_generation").await);
}

#[tokio::test]
async fn test_v2_library_items_has_freshness_columns() {
    let store = fresh_store().await;
    assert!(column_exists(&store, "library_items", "freshness_class").await);
    assert!(column_exists(&store, "library_items", "sync_generation").await);
}

#[tokio::test]
async fn test_check_cache_version_reports_current_at_v2() {
    let store = fresh_store().await;
    let v = store.applied_cache_version().await.unwrap();
    assert_eq!(v, CACHE_VERSION as i64);
}

#[tokio::test]
async fn test_check_cache_version_returns_too_new_when_db_ahead() {
    let store = fresh_store().await;
    // Simulate a future migration applied row.
    sqlx::query("INSERT INTO schema_migrations (version, name, applied_at_ms) VALUES (?, 'future', 0)")
        .bind(99_i64)
        .execute(store.writer_for_test())
        .await
        .unwrap();

    match store.check_cache_version().await {
        Err(message) => {
            let s = message.to_string();
            assert!(s.contains("99"), "error must mention future version: {s}");
        }
        Ok(()) => panic!("expected check_cache_version to error on future version"),
    }
}

#[tokio::test]
async fn test_check_cache_version_clean_at_current() {
    let store = fresh_store().await;
    store.check_cache_version().await.expect("v2 store should be ok");
}
