//! Phase 8 — connect spotuify-mcp to a running spotuify daemon.
//!
//! Sends a typed `spotuify_protocol::Request` over the daemon IPC stream
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
use spotuify_protocol::{
    default_socket_path as protocol_socket_path, IpcCodec, IpcMessage, IpcPayload, OperationSource,
    Request, Response,
};
use tokio_util::codec::Framed;

/// Default daemon IPC address.
pub fn default_socket_path() -> PathBuf {
    protocol_socket_path()
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
        spotuify_protocol::ipc_stream::connect(socket_path),
    )
    .await
    .map_err(|_| anyhow!("timed out connecting to daemon IPC {socket_path:?}"))?
    .with_context(|| format!("connect to daemon IPC {socket_path:?}"))?;

    let mut framed = Framed::new(stream, IpcCodec::new());

    let envelope = IpcMessage {
        id: 1,
        source: Some(OperationSource::Mcp),
        payload: IpcPayload::Request(request),
    };
    framed
        .send(envelope)
        .await
        .context("send Request over daemon IPC")?;

    let resp = tokio::time::timeout(REQUEST_TIMEOUT, framed.next())
        .await
        .map_err(|_| anyhow!("daemon did not respond within {REQUEST_TIMEOUT:?}"))?;

    match resp {
        Some(Ok(msg)) => match msg.payload {
            IpcPayload::Response(r) => Ok(r),
            other => Err(anyhow!(
                "daemon sent unexpected payload {:?}",
                payload_kind(&other)
            )),
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
    use std::sync::Mutex;

    // Process-wide env is shared across parallel cargo tests; serialise
    // the env-mutating socket-path tests through a single mutex.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn default_socket_path_honours_env_override() {
        let _g = ENV_LOCK.lock().expect("env lock should not be poisoned");
        std::env::set_var("SPOTUIFY_SOCKET", "/tmp/spotuify-test.sock");
        let p = default_socket_path();
        assert_eq!(p, PathBuf::from("/tmp/spotuify-test.sock"));
        std::env::remove_var("SPOTUIFY_SOCKET");
    }

    #[test]
    fn default_socket_path_uses_shared_runtime_resolver() {
        let _g = ENV_LOCK.lock().expect("env lock should not be poisoned");
        std::env::remove_var("SPOTUIFY_SOCKET");
        std::env::set_var("SPOTUIFY_RUNTIME_DIR", "/tmp/spotuify-runtime-test");
        let p = default_socket_path();
        #[cfg(unix)]
        assert_eq!(p, PathBuf::from("/tmp/spotuify-runtime-test/daemon.sock"));
        #[cfg(windows)]
        assert!(
            p.to_string_lossy().starts_with(r"\\.\pipe\"),
            "windows IPC should use a named-pipe address"
        );
        std::env::remove_var("SPOTUIFY_RUNTIME_DIR");
    }
}
