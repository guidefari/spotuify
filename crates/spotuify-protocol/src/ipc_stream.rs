//! Cross-platform IPC stream abstraction.
//!
//! Unix builds use Unix-domain sockets. Windows builds use Tokio named
//! pipes behind the same async read/write stream type so the daemon,
//! CLI, and MCP bridge share one codec path.

use std::io;
use std::path::Path;
#[cfg(windows)]
use std::path::PathBuf;

use tokio::io::{AsyncRead, AsyncWrite};

pub trait IpcReadWrite: AsyncRead + AsyncWrite + Unpin + Send {}

impl<T> IpcReadWrite for T where T: AsyncRead + AsyncWrite + Unpin + Send {}

pub type IpcStream = Box<dyn IpcReadWrite>;

#[cfg(unix)]
pub struct IpcListener {
    inner: tokio::net::UnixListener,
}

#[cfg(unix)]
impl IpcListener {
    pub fn bind(path: &Path) -> io::Result<Self> {
        tokio::net::UnixListener::bind(path).map(|inner| Self { inner })
    }

    pub async fn accept(&mut self) -> io::Result<IpcStream> {
        let (stream, _) = self.inner.accept().await?;
        Ok(Box::new(stream))
    }
}

#[cfg(unix)]
pub async fn connect(path: &Path) -> io::Result<IpcStream> {
    tokio::net::UnixStream::connect(path)
        .await
        .map(|stream| Box::new(stream) as IpcStream)
}

#[cfg(windows)]
pub struct IpcListener {
    path: PathBuf,
    pending: tokio::net::windows::named_pipe::NamedPipeServer,
}

#[cfg(windows)]
impl IpcListener {
    pub fn bind(path: &Path) -> io::Result<Self> {
        let pending = create_server(path)?;
        Ok(Self {
            path: path.to_path_buf(),
            pending,
        })
    }

    pub async fn accept(&mut self) -> io::Result<IpcStream> {
        self.pending.connect().await?;
        let next = create_server(&self.path)?;
        let connected = std::mem::replace(&mut self.pending, next);
        Ok(Box::new(connected))
    }
}

#[cfg(windows)]
fn create_server(path: &Path) -> io::Result<tokio::net::windows::named_pipe::NamedPipeServer> {
    tokio::net::windows::named_pipe::ServerOptions::new()
        .first_pipe_instance(false)
        .create(path)
}

#[cfg(windows)]
pub async fn connect(path: &Path) -> io::Result<IpcStream> {
    use tokio::net::windows::named_pipe::ClientOptions;

    const ERROR_FILE_NOT_FOUND: i32 = 2;
    const ERROR_PIPE_BUSY: i32 = 231;
    const ATTEMPTS: usize = 20;
    const DELAY: std::time::Duration = std::time::Duration::from_millis(50);

    for attempt in 0..ATTEMPTS {
        match ClientOptions::new().open(path) {
            Ok(client) => return Ok(Box::new(client)),
            Err(err)
                if attempt + 1 < ATTEMPTS
                    && matches!(
                        err.raw_os_error(),
                        Some(ERROR_FILE_NOT_FOUND | ERROR_PIPE_BUSY)
                    ) =>
            {
                tokio::time::sleep(DELAY).await;
            }
            Err(err) => return Err(err),
        }
    }

    unreachable!("loop either returns a client or the last open error")
}
