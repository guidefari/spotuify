#![allow(clippy::panic, clippy::unwrap_used)]

use assert_cmd::Command;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
use std::thread::sleep;
use std::time::Duration;
use tempfile::TempDir;

struct DaemonGuard {
    socket_path: PathBuf,
    pid: Option<u64>,
}

impl Drop for DaemonGuard {
    fn drop(&mut self) {
        if let Some(pid) = self.pid {
            let pid = pid.to_string();
            let _ = StdCommand::new("kill").arg(&pid).status();
            let mut stopped = false;
            for _ in 0..40 {
                if !self.socket_path.exists() {
                    stopped = true;
                    break;
                }
                sleep(Duration::from_millis(50));
            }
            // SIGTERM didn't take in time — don't leave it running.
            if !stopped {
                let _ = StdCommand::new("kill").args(["-KILL", &pid]).status();
            }
        }
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

#[test]
fn fake_daemon_cli_journey_covers_json_ids_and_mutation_receipts() {
    let temp = TempDir::new().expect("temp dir");
    let socket_path = temp.path().join("runtime/daemon.sock");
    let mut daemon = DaemonGuard {
        socket_path: socket_path.clone(),
        pid: None,
    };

    let devices = run_json(temp.path(), &["devices", "--format", "json"]);
    let status = run_json(temp.path(), &["daemon", "status", "--format", "json"]);
    daemon.pid = status["daemon_pid"].as_u64();
    assert!(
        daemon.pid.is_some(),
        "fake daemon should be resident: {status:#}"
    );
    assert_eq!(devices[0]["name"].as_str(), Some("spotuify-fake"));
    assert_eq!(devices[0]["is_active"].as_bool(), Some(true));

    let search = run_json(
        temp.path(),
        &[
            "search",
            "luther vandross",
            "--type",
            "track",
            "--format",
            "json",
        ],
    );
    assert_eq!(
        search[0]["uri"].as_str(),
        Some("spotify:track:never-too-much")
    );
    assert_eq!(search[0]["kind"].as_str(), Some("track"));

    let ids = run_stdout(
        temp.path(),
        &[
            "search",
            "luther vandross",
            "--type",
            "track",
            "--format",
            "ids",
        ],
    );
    assert_eq!(ids, "spotify:track:never-too-much\n");

    let receipt = run_json(
        temp.path(),
        &[
            "queue",
            "add",
            "spotify:track:never-too-much",
            "--format",
            "json",
        ],
    );
    assert_eq!(receipt["ok"].as_bool(), Some(true));
    assert_eq!(receipt["action"].as_str(), Some("queue"));
}

#[test]
fn fake_daemon_accepts_batch_ids_for_queue_and_playlist_preview() {
    let temp = TempDir::new().expect("temp dir");
    let ids_path = temp.path().join("tracks.txt");
    std::fs::write(
        &ids_path,
        "spotify:track:never-too-much\nspotify:track:sweet-thing\n",
    )
    .expect("write ids file");

    let queue = run_json(
        temp.path(),
        &[
            "queue",
            "add",
            "--ids",
            ids_path.to_str().expect("utf8 path"),
            "--format",
            "json",
        ],
    );
    assert_eq!(queue["ok"].as_bool(), Some(true));
    assert_eq!(queue["action"].as_str(), Some("queue"));
    assert_eq!(queue["requested"].as_u64(), Some(2));
    assert_eq!(queue["succeeded"].as_u64(), Some(2));
    assert_eq!(
        queue["uris"][0].as_str(),
        Some("spotify:track:never-too-much")
    );

    let preview = run_json(
        temp.path(),
        &[
            "playlist",
            "add",
            "quiet-storm",
            "--ids",
            ids_path.to_str().expect("utf8 path"),
            "--dry-run",
            "--format",
            "json",
        ],
    );
    assert_eq!(preview["ok"].as_bool(), Some(true));
    assert_eq!(preview["action"].as_str(), Some("playlist-add"));
    assert_eq!(preview["dry_run"].as_bool(), Some(true));
    assert_eq!(preview["requested"].as_u64(), Some(2));
    assert_eq!(preview["succeeded"].as_u64(), Some(0));
    assert_eq!(preview["playlist"].as_str(), Some("quiet-storm"));
}

#[test]
fn fake_daemon_accepts_stdin_ids_for_queue() {
    let temp = TempDir::new().expect("temp dir");
    let output = command(temp.path())
        .args(["queue", "add", "--format", "ids"])
        .write_stdin("spotify:track:never-too-much\nspotify:track:sweet-thing\n")
        .assert()
        .success()
        .get_output()
        .clone();

    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert_eq!(
        stdout,
        "spotify:track:never-too-much\nspotify:track:sweet-thing\n"
    );
}

#[test]
fn playlist_batch_commit_requires_yes_outside_dry_run() {
    let temp = TempDir::new().expect("temp dir");
    let output = command(temp.path())
        .args([
            "playlist",
            "add",
            "quiet-storm",
            "spotify:track:never-too-much",
            "spotify:track:sweet-thing",
            "--format",
            "json",
        ])
        .assert()
        .code(1)
        .get_output()
        .clone();

    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.contains("Re-run with --yes or inspect with --dry-run"),
        "unsafe batch mutation should fail closed, got {stderr:?}"
    );
}

fn run_json(root: &Path, args: &[&str]) -> Value {
    let stdout = run_stdout(root, args);
    serde_json::from_str(stdout.trim()).unwrap_or_else(|err| {
        panic!(
            "expected JSON from `spotuify {}`: {err}\nstdout={stdout}",
            args.join(" ")
        )
    })
}

fn run_stdout(root: &Path, args: &[&str]) -> String {
    let output = command(root)
        .args(args)
        .assert()
        .success()
        .get_output()
        .clone();
    String::from_utf8(output.stdout).expect("utf8 stdout")
}

fn command(root: &Path) -> Command {
    let runtime_dir = root.join("runtime");
    let mut command = Command::cargo_bin("spotuify").expect("spotuify binary");
    command
        .env("SPOTUIFY_FAKE_SPOTIFY", "1")
        // Tie any auto-started daemon's lifetime to this test process so a
        // killed `cargo test`/`nextest` run can't leave an orphaned daemon.
        .env("SPOTUIFY_EXIT_WITH_PARENT", std::process::id().to_string())
        .env("SPOTUIFY_RUNTIME_DIR", &runtime_dir)
        .env("SPOTUIFY_SOCKET", runtime_dir.join("daemon.sock"))
        .env("SPOTUIFY_CACHE_DB", root.join("cache.sqlite"))
        .env("SPOTUIFY_SEARCH_INDEX", root.join("index"))
        .env("SPOTUIFY_ANALYTICS_DB", root.join("analytics.sqlite"))
        .env("SPOTUIFY_CONFIG", root.join("spotuify.toml"));
    command
}
