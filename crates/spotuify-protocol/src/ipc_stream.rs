//! Phase 11 — cross-platform IPC stream abstraction.
//!
//! Today (Pass 1 / F7 scaffold) this module exposes target-conditional
//! type aliases for the daemon's listener + per-connection stream type.
//! On Unix it's a thin alias over `tokio::net::Unix*`; on Windows it
//! aliases over `tokio::net::windows::named_pipe::*`.
//!
//! Pass 2 (P11) replaces the aliases with concrete enum wrappers + a
//! `bind(path: &Path) -> Result<IpcListener>` factory that routes the
//! caller-provided socket path to the right transport. The aliases
//! pre-stage the abstraction so call sites can migrate gradually
//! without breaking macOS today.

#[cfg(unix)]
pub type IpcListener = tokio::net::UnixListener;

#[cfg(unix)]
pub type IpcStream = tokio::net::UnixStream;

// Windows uses named pipes. NamedPipeServer is a listener-shaped half
// of one pipe instance; in the full Pass 2 impl, the daemon allocates
// new server instances ahead of each incoming client just like a
// Unix-domain socket accept loop.
#[cfg(windows)]
pub type IpcListener = tokio::net::windows::named_pipe::NamedPipeServer;

#[cfg(windows)]
pub type IpcStream = tokio::net::windows::named_pipe::NamedPipeClient;
