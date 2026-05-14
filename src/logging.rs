//! Re-export bridge for spotuify-daemon::logging.
pub use spotuify_daemon::logging::*;

use std::fs;
use std::io::Write;
use std::path::PathBuf;

/// Phase 13 (P13-B) — write a backtrace file when the process panics.
/// Lands under `~/.cache/spotuify/backtrace/<unix_ts>-<pid>.log` (or
/// macOS-equivalent) so the next start can surface "previous run
/// crashed — see <path>" without losing the trace to the TUI altscreen.
pub fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let backtrace = std::backtrace::Backtrace::force_capture();
        let payload = format!(
            "spotuify panic at {now}\n\
             pid: {pid}\n\
             version: {version}\n\
             location: {loc}\n\
             payload: {payload}\n\
             \n\
             backtrace:\n{bt}\n",
            now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            pid = std::process::id(),
            version = env!("CARGO_PKG_VERSION"),
            loc = info
                .location()
                .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
                .unwrap_or_else(|| "<unknown>".to_string()),
            payload = panic_payload(info),
            bt = backtrace,
        );

        // Best-effort write. If the cache dir is unavailable we still
        // delegate to the default hook so the panic isn't silently
        // swallowed.
        if let Some(path) = backtrace_log_path() {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Ok(mut file) = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
            {
                let _ = file.write_all(payload.as_bytes());
                eprintln!("spotuify panicked. trace: {}", path.display());
            }
        }
        default_hook(info);
    }));
}

/// Resolve the backtrace log path. One file per panic via timestamp+pid
/// so concurrent panics across multiple processes don't trample.
pub fn backtrace_log_path() -> Option<PathBuf> {
    let dir = backtrace_dir()?;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Some(dir.join(format!("{ts}-{pid}.log", pid = std::process::id())))
}

pub fn backtrace_dir() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        dirs::home_dir().map(|h| h.join("Library/Caches/spotuify/backtrace"))
    } else {
        dirs::cache_dir()
            .or_else(|| dirs::home_dir().map(|h| h.join(".cache")))
            .map(|d| d.join("spotuify/backtrace"))
    }
}

/// Phase 13 (P13-B) — surface a "previous run crashed" warning on next
/// start. Called immediately after tracing init so the warning lands in
/// the freshly-rotated log.
pub fn surface_prior_panic_if_any() {
    let Some(dir) = backtrace_dir() else { return };
    let Ok(entries) = fs::read_dir(&dir) else { return };
    let mut latest: Option<(u64, PathBuf)> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        let modified = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        match latest {
            Some((m, _)) if modified > m => latest = Some((modified, path)),
            None => latest = Some((modified, path)),
            _ => {}
        }
    }
    if let Some((_, path)) = latest {
        tracing::warn!(
            backtrace = %path.display(),
            "previous run wrote a panic backtrace — inspect this file before retrying"
        );
    }
}

fn panic_payload(info: &std::panic::PanicHookInfo<'_>) -> String {
    if let Some(s) = info.payload().downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = info.payload().downcast_ref::<String>() {
        s.clone()
    } else {
        "<non-string payload>".to_string()
    }
}
