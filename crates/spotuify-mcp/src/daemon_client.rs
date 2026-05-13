//! Phase 8 — connect spotuify-mcp to a running spotuify daemon.
//!
//! Sends a typed `spotuify_protocol::Request` over the Unix socket
//! and returns the `Response`. Used by `tools/call` to actually
//! execute mutations after the catalogue + confirm gating and
//! bridge translation.
//!
//! Async client; the rpc dispatch is sync because MCP is line-at-a-time.
//! The stdio loop wraps each tools/call in a tokio current-thread
//! runtime to bridge.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use futures::{SinkExt, StreamExt};
use spotuify_protocol::{IpcCodec, IpcMessage, IpcPayload, Request, Response};
use tokio::net::UnixStream;
use tokio_util::codec::Framed;

/// Default daemon socket path. The daemon writes here at startup.
pub fn default_socket_path() -> PathBuf {
    if let Some(custom) = std::env::var_os("SPOTUIFY_SOCKET") {
        return PathBuf::from(custom);
    }
    dirs::cache_dir()
        .unwrap_or_else(|| std::env::temp_dir())
        .join("spotuify/daemon.sock")
}

const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// Round-trip a single Request against the daemon.
///
/// Returns Err if the daemon isn't reachable (missing socket / hung).
/// Successful protocol exchanges -- including daemon-side error
/// envelopes -- come back as Ok(Response::Error { .. }).
pub async fn round_trip(socket_path: &Path, request: Request) -> Result<Response> {
    let stream = tokio::time::timeout(
        Duration::from_secs(2),
        UnixStream::connect(socket_path),
    )
    .await
    .map_err(|_| anyhow!("timed out connecting to daemon socket {socket_path:?}"))?
    .with_context(|| format!("connect to daemon socket {socket_path:?}"))?;

    let mut framed = Framed::new(stream, IpcCodec::new());

    let envelope = IpcMessage {
        id: 1,
        payload: IpcPayload::Request(request),
    };
    framed
        .send(envelope)
        .await
        .context("send Request over daemon socket")?;

    let resp = tokio::time::timeout(REQUEST_TIMEOUT, framed.next())
        .await
        .map_err(|_| anyhow!("daemon did not respond within {:?}", REQUEST_TIMEOUT))?;

    match resp {
        Some(Ok(msg)) => match msg.payload {
            IpcPayload::Response(r) => Ok(r),
            other => Err(anyhow!("daemon sent unexpected payload {:?}", payload_kind(&other))),
        },
        Some(Err(err)) => Err(anyhow!("daemon stream decode error: {err}")),
        None => Err(anyhow!("daemon closed the connection without responding")),
    }
}

fn payload_kind(p: &IpcPayload) -> &'static str {
    match p {
        IpcPayload::Request(_) => "request",
        IpcPayload::Response(_) => "response",
        IpcPayload::Event(_) => "event",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_socket_path_honours_env_override() {
        std::env::set_var("SPOTUIFY_SOCKET", "/tmp/spotuify-test.sock");
        let p = default_socket_path();
        assert_eq!(p, PathBuf::from("/tmp/spotuify-test.sock"));
        std::env::remove_var("SPOTUIFY_SOCKET");
    }

    #[test]
    fn default_socket_path_falls_back_to_cache_dir() {
        std::env::remove_var("SPOTUIFY_SOCKET");
        let p = default_socket_path();
        assert!(
            p.to_string_lossy().ends_with("spotuify/daemon.sock"),
            "expected cache-dir suffix, got {p:?}"
        );
    }
}
