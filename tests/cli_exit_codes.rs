use assert_cmd::Command;

fn isolated_runtime() -> tempfile::TempDir {
    tempfile::tempdir().expect("tempdir")
}

#[test]
fn missing_queue_target_exits_with_usage_code() {
    let output = Command::cargo_bin("spotuify")
        .expect("spotuify binary")
        .args(["queue", "add"])
        .assert()
        .code(2)
        .get_output()
        .clone();

    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.contains("provide a URI or --search QUERY"),
        "usage error should explain the missing queue target, got {stderr:?}"
    );
}

#[test]
fn no_daemon_start_status_fails_without_spawning_daemon() {
    let runtime = isolated_runtime();
    let output = Command::cargo_bin("spotuify")
        .expect("spotuify binary")
        .env("SPOTUIFY_RUNTIME_DIR", runtime.path())
        .env("SPOTUIFY_SOCKET", runtime.path().join("daemon.sock"))
        .args(["--no-daemon-start", "status"])
        .assert()
        .failure()
        .get_output()
        .clone();

    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.contains("daemon not running") && stderr.contains("--no-daemon-start"),
        "no-daemon-start should fail with a clear daemon hint, got {stderr:?}"
    );
    assert!(
        !runtime.path().join("daemon.sock").exists(),
        "status must not spawn a daemon when --no-daemon-start is set"
    );
}

#[test]
fn cache_reset_without_confirm_exits_with_usage_code() {
    let output = Command::cargo_bin("spotuify")
        .expect("spotuify binary")
        .args(["cache", "reset"])
        .assert()
        .code(2)
        .get_output()
        .clone();

    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.contains("cache reset is destructive; re-run with --confirm"),
        "destructive cache reset should fail closed, got {stderr:?}"
    );
}
