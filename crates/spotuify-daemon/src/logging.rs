use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;

/// Phase 13 (P13-A) — log output format. Plaintext is the default;
/// JSON is opt-in via `SPOTUIFY_LOG_FORMAT=json` or `--log-format json`.
/// JSON output is what agents and `spotuify logs tail --format json`
/// consume.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    Text,
    Json,
}

impl LogFormat {
    pub fn from_env_or_default() -> Self {
        match std::env::var("SPOTUIFY_LOG_FORMAT").ok().as_deref() {
            Some("json") | Some("jsonl") => Self::Json,
            _ => Self::Text,
        }
    }
}

pub fn init() -> Result<WorkerGuard> {
    init_with_format(LogFormat::from_env_or_default())
}

pub fn init_with_format(format: LogFormat) -> Result<WorkerGuard> {
    let path = log_path()?;
    let dir = path
        .parent()
        .context("log path has no parent directory")?
        .to_path_buf();
    fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;

    let appender = tracing_appender::rolling::never(&dir, "spotuify.log");
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let filter = EnvFilter::try_from_env("SPOTUIFY_LOG")
        .unwrap_or_else(|_| EnvFilter::new("spotuify=debug,info"));

    match format {
        LogFormat::Json => {
            let _ = tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_writer(writer)
                .with_ansi(false)
                .json()
                .with_current_span(true)
                .with_span_list(false)
                .try_init();
        }
        LogFormat::Text => {
            let _ = tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_writer(writer)
                .with_ansi(false)
                .try_init();
        }
    }

    Ok(guard)
}

pub fn log_path() -> Result<PathBuf> {
    if cfg!(target_os = "macos") {
        return dirs::home_dir()
            .map(|home| home.join("Library/Logs/spotuify/spotuify.log"))
            .context("could not resolve home directory");
    }

    dirs::cache_dir()
        .or_else(|| dirs::home_dir().map(|home| home.join(".cache")))
        .map(|dir| dir.join("spotuify/spotuify.log"))
        .context("could not resolve cache directory")
}

pub fn read_tail(lines: usize) -> Result<String> {
    let path = log_path()?;
    if !path.exists() {
        return Ok(String::new());
    }

    let contents =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let lines = contents
        .lines()
        .rev()
        .take(lines)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");
    Ok(lines)
}
