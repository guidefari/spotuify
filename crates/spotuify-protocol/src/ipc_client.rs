use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{bail, Result};
use futures::{SinkExt, StreamExt};
use tokio::net::UnixStream;
use tokio::time::timeout;
use tokio_util::codec::Framed;

use crate::{DaemonEvent, IpcCodec, IpcMessage, IpcPayload, OperationSource, Request, Response};

const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Default daemon socket path. Same resolution logic as the daemon
/// uses to pick where to bind; honours `SPOTUIFY_SOCKET` override.
pub fn default_socket_path() -> std::path::PathBuf {
    crate::paths::socket_path()
}

pub struct IpcClient {
    framed: Framed<UnixStream, IpcCodec>,
    next_id: AtomicU64,
    source: Option<OperationSource>,
    events_subscribed: bool,
}

impl IpcClient {
    pub async fn connect() -> Result<Self> {
        Self::connect_to(&default_socket_path()).await
    }

    pub async fn connect_with_source(source: OperationSource) -> Result<Self> {
        Self::connect_to_with_source(&default_socket_path(), source).await
    }

    pub async fn connect_to(socket_path: &Path) -> Result<Self> {
        Self::connect_to_inner(socket_path, None).await
    }

    pub async fn connect_to_with_source(
        socket_path: &Path,
        source: OperationSource,
    ) -> Result<Self> {
        Self::connect_to_inner(socket_path, Some(source)).await
    }

    async fn connect_to_inner(socket_path: &Path, source: Option<OperationSource>) -> Result<Self> {
        let stream = UnixStream::connect(socket_path).await.map_err(|err| {
            anyhow::anyhow!(
                "Cannot connect to daemon at {}: {}. Try: spotuify daemon start",
                socket_path.display(),
                err
            )
        })?;
        Ok(Self {
            framed: Framed::new(stream, IpcCodec::new()),
            next_id: AtomicU64::new(1),
            source,
            events_subscribed: false,
        })
    }

    pub async fn request(&mut self, request: Request) -> Result<Response> {
        self.request_with_timeout(request, DEFAULT_REQUEST_TIMEOUT)
            .await
    }

    pub async fn request_with_timeout(
        &mut self,
        request: Request,
        duration: Duration,
    ) -> Result<Response> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.framed
            .send(IpcMessage {
                id,
                source: self.source,
                payload: IpcPayload::Request(request),
            })
            .await?;

        timeout(duration, async {
            loop {
                match self.framed.next().await {
                    Some(Ok(message)) => match message.payload {
                        IpcPayload::Response(response) if message.id == id => return Ok(response),
                        IpcPayload::Event(_event) => {}
                        IpcPayload::Response(_) => bail!(
                            "IPC protocol error: received response id {} while waiting for {id}",
                            message.id
                        ),
                        IpcPayload::Request(_) => bail!(
                            "IPC protocol error: received request while waiting for response {id}"
                        ),
                    },
                    Some(Err(err)) => bail!("{}", describe_ipc_failure(&err.to_string())),
                    None => bail!(
                        "Connection closed. Restart the daemon after upgrading: spotuify daemon restart"
                    ),
                }
            }
        })
        .await
        .map_err(|_| anyhow::anyhow!("IPC request timed out after {}", format_timeout(duration)))?
    }

    pub async fn next_event(&mut self) -> Result<DaemonEvent> {
        self.subscribe_events().await?;
        loop {
            match self.framed.next().await {
                Some(Ok(message)) => match message.payload {
                    IpcPayload::Event(event) => return Ok(event),
                    IpcPayload::Response(_) | IpcPayload::Request(_) => {}
                },
                Some(Err(err)) => bail!("{}", describe_ipc_failure(&err.to_string())),
                None => bail!(
                    "Connection closed. Restart the daemon after upgrading: spotuify daemon restart"
                ),
            }
        }
    }

    async fn subscribe_events(&mut self) -> Result<()> {
        if self.events_subscribed {
            return Ok(());
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.framed
            .send(IpcMessage {
                id,
                source: self.source,
                payload: IpcPayload::Request(Request::SubscribeEvents),
            })
            .await?;
        self.events_subscribed = true;
        Ok(())
    }
}

fn format_timeout(duration: Duration) -> String {
    if duration.as_millis() < 1_000 {
        format!("{}ms", duration.as_millis())
    } else {
        format!("{}s", duration.as_secs())
    }
}

fn describe_ipc_failure(message: &str) -> String {
    if message.contains("unknown variant") || message.contains("missing field") {
        format!("IPC protocol mismatch: {message}. Restart the daemon after upgrading.")
    } else {
        format!("IPC error: {message}")
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use futures::{SinkExt, StreamExt};
    use tempfile::TempDir;
    use tokio::net::UnixListener;
    use tokio_util::codec::Framed;

    use super::{default_socket_path, IpcClient};
    use crate::{DaemonEvent, IpcCodec, IpcMessage, IpcPayload, Request, Response, ResponseData};

    #[test]
    fn default_socket_path_uses_shared_runtime_resolver() {
        let _guard = crate::TEST_ENV_LOCK
            .lock()
            .expect("env lock should not be poisoned");
        std::env::remove_var("SPOTUIFY_SOCKET");
        std::env::set_var("SPOTUIFY_RUNTIME_DIR", "/tmp/spotuify-runtime-test");

        assert_eq!(
            default_socket_path(),
            PathBuf::from("/tmp/spotuify-runtime-test/daemon.sock")
        );

        std::env::remove_var("SPOTUIFY_RUNTIME_DIR");
    }

    #[tokio::test]
    async fn request_with_timeout_returns_actionable_error_when_daemon_stalls() {
        let temp = TempDir::new().unwrap();
        let socket = temp.path().join("stall.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.unwrap();
            tokio::time::sleep(Duration::from_millis(200)).await;
        });
        let mut client = IpcClient::connect_to(&socket).await.unwrap();

        let err = client
            .request_with_timeout(Request::Ping, Duration::from_millis(20))
            .await
            .unwrap_err();

        assert!(
            err.to_string().contains("IPC request timed out after 20ms"),
            "timeout should be surfaced to callers, got {err:#}"
        );
    }

    #[tokio::test]
    async fn request_ignores_events_until_matching_response_arrives() {
        let temp = TempDir::new().unwrap();
        let socket = temp.path().join("events.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut framed = Framed::new(stream, IpcCodec::new());
            let request = framed.next().await.unwrap().unwrap();
            framed
                .send(IpcMessage {
                    id: 0,
                    source: None,
                    payload: IpcPayload::Event(DaemonEvent::ShutdownRequested),
                })
                .await
                .unwrap();
            framed
                .send(IpcMessage {
                    id: request.id,
                    source: None,
                    payload: IpcPayload::Response(Response::Ok {
                        data: ResponseData::Pong,
                    }),
                })
                .await
                .unwrap();
        });
        let mut client = IpcClient::connect_to(&socket).await.unwrap();

        let response = client
            .request_with_timeout(Request::Ping, Duration::from_secs(1))
            .await
            .unwrap();

        assert!(matches!(
            response,
            Response::Ok {
                data: ResponseData::Pong
            }
        ));
    }

    #[tokio::test]
    async fn request_sends_configured_operation_source() {
        let temp = TempDir::new().expect("temp dir should be created");
        let socket = temp.path().join("source.sock");
        let listener = UnixListener::bind(&socket).expect("listener should bind");
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("client should connect");
            let mut framed = Framed::new(stream, IpcCodec::new());
            let request = framed
                .next()
                .await
                .expect("client should send a frame")
                .expect("request frame should decode");
            assert_eq!(request.source, Some(crate::OperationSource::Tui));
            framed
                .send(IpcMessage {
                    id: request.id,
                    source: None,
                    payload: IpcPayload::Response(Response::Ok {
                        data: ResponseData::Pong,
                    }),
                })
                .await
                .expect("response should send");
        });
        let mut client = IpcClient::connect_to_with_source(&socket, crate::OperationSource::Tui)
            .await
            .expect("client should connect");

        let response = client
            .request_with_timeout(Request::Ping, Duration::from_secs(1))
            .await
            .expect("request should receive pong");

        assert!(matches!(
            response,
            Response::Ok {
                data: ResponseData::Pong
            }
        ));
    }

    #[tokio::test]
    async fn next_event_returns_broadcast_daemon_events() {
        let temp = TempDir::new().unwrap();
        let socket = temp.path().join("event-stream.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut framed = Framed::new(stream, IpcCodec::new());
            let subscribe = framed.next().await.unwrap().unwrap();
            assert!(matches!(
                subscribe.payload,
                IpcPayload::Request(Request::SubscribeEvents)
            ));
            framed
                .send(IpcMessage {
                    id: 0,
                    source: None,
                    payload: IpcPayload::Event(DaemonEvent::QueueChanged {
                        action: "queue".to_string(),
                        uris: vec!["spotify:track:1".to_string()],
                        queue: None,
                    }),
                })
                .await
                .unwrap();
        });
        let mut client = IpcClient::connect_to(&socket).await.unwrap();

        let event = client.next_event().await.unwrap();

        assert_eq!(
            event,
            DaemonEvent::QueueChanged {
                action: "queue".to_string(),
                uris: vec!["spotify:track:1".to_string()],
                queue: None,
            }
        );
    }
}
