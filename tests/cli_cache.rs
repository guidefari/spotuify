use assert_cmd::Command;

fn command(root: &std::path::Path) -> Command {
    let runtime_dir = root.join("runtime");
    let mut command = Command::cargo_bin("spotuify").expect("spotuify binary");
    command
        // Tie any auto-started daemon's lifetime to this test process so a
        // killed `cargo test`/`nextest` run can't leave an orphaned daemon.
        .env("SPOTUIFY_EXIT_WITH_PARENT", std::process::id().to_string())
        .env("SPOTUIFY_RUNTIME_DIR", &runtime_dir)
        .env("SPOTUIFY_SOCKET", runtime_dir.join("daemon.sock"))
        .env("SPOTUIFY_CACHE_DB", root.join("cache.sqlite3"))
        .env("SPOTUIFY_SEARCH_INDEX", root.join("index"));
    command
}

#[test]
fn cache_reset_confirm_deletes_database_sidecars_and_index() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let db = temp.path().join("cache.sqlite3");
    let wal = temp.path().join("cache.sqlite3-wal");
    let shm = temp.path().join("cache.sqlite3-shm");
    let index = temp.path().join("index");
    std::fs::write(&db, "db").expect("db");
    std::fs::write(&wal, "wal").expect("wal");
    std::fs::write(&shm, "shm").expect("shm");
    std::fs::create_dir_all(&index).expect("index dir");
    std::fs::write(index.join("segment"), "index").expect("index segment");

    let output = command(temp.path())
        .args(["cache", "reset", "--confirm", "--format", "json"])
        .assert()
        .success()
        .get_output()
        .clone();

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("cache reset should print JSON");
    assert_eq!(json["ok"].as_bool(), Some(true));
    assert_eq!(json["action"].as_str(), Some("cache-reset"));
    assert!(!db.exists());
    assert!(!wal.exists());
    assert!(!shm.exists());
    assert!(!index.exists());
}

#[test]
fn cache_repair_recreates_empty_cache_and_index() {
    let temp = tempfile::TempDir::new().expect("temp dir");

    let output = command(temp.path())
        .args(["cache", "repair", "--format", "json"])
        .assert()
        .success()
        .get_output()
        .clone();

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("cache repair should print JSON");
    assert_eq!(json["indexed"].as_u64(), Some(0));
    assert!(temp.path().join("cache.sqlite3").exists());
    assert!(temp.path().join("index").exists());
}
