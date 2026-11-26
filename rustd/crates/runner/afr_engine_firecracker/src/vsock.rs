//! How the host reaches a guest's executor: the monitor's vsock Unix socket,
//! then a `CONNECT <port>` line, which the monitor answers `OK <port>` once a
//! guest process listens on that port, and closes otherwise.
//!
//! A guest that is still booting has no listener yet, so a dial retries until
//! the guest answers or the boot's time is up. Each boot is a fresh guest,
//! and the executor client dials again after every one (`afr_executor`'s
//! [`Transport`]).

use std::io;
use std::path::PathBuf;
use std::time::Duration;

use afr_executor::transport::{Stream, Transport};
use backon::{ConstantBuilder, Retryable as _};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::UnixStream;

/// What the monitor's answer starts with when a guest listens on the port.
const ACCEPTED: &str = "OK ";
/// The longest answer the monitor gives: `OK `, a 32-bit port in decimal and
/// its newline, as Firecracker's hybrid vsock writes it.
const REPLY_BYTES_MAX: usize = ACCEPTED.len() + 10 + 1;
/// The pause between two dials of a guest that is not listening yet.
const DIAL_RETRY_DELAY: Duration = Duration::from_millis(10);
/// What a guest that closed the dial without an answer is reported as.
const UNANSWERED: &str = "the guest's monitor closed the connection without accepting it";

/// One guest's executor, through its monitor's vsock socket.
#[derive(Debug, Clone)]
pub struct VsockTransport {
    socket: PathBuf,
    port: u32,
    within: Duration,
}

impl VsockTransport {
    /// The executor on guest `port`, through the monitor socket at `socket`,
    /// each dial retried until the guest answers or `within` has passed.
    #[must_use]
    pub const fn new(socket: PathBuf, port: u32, within: Duration) -> Self {
        Self {
            socket,
            port,
            within,
        }
    }
}

#[async_trait::async_trait]
impl Transport for VsockTransport {
    async fn dial(&self) -> io::Result<Box<dyn Stream>> {
        let attempts = self
            .within
            .as_millis()
            .div_euclid(DIAL_RETRY_DELAY.as_millis());
        let policy = ConstantBuilder::new()
            .with_delay(DIAL_RETRY_DELAY)
            .with_max_times(usize::try_from(attempts).unwrap_or(usize::MAX));
        let stream = (|| connect(&self.socket, self.port))
            .retry(policy)
            .when(booting)
            .await?;
        Ok(Box::new(stream))
    }
}

/// Dials the monitor at `socket` and asks it for guest `port`.
///
/// # Errors
/// The socket is not there or refuses, or the monitor answers anything but
/// its acceptance.
async fn connect(socket: &std::path::Path, port: u32) -> io::Result<UnixStream> {
    let mut stream = UnixStream::connect(socket).await?;
    stream
        .write_all(format!("CONNECT {port}\n").as_bytes())
        .await?;
    // One byte at a time: what follows the newline is the executor's.
    let mut reply = Vec::with_capacity(REPLY_BYTES_MAX);
    while reply.len() < REPLY_BYTES_MAX {
        match stream.read_u8().await {
            Ok(b'\n') => break,
            Ok(byte) => reply.push(byte),
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
                return Err(io::Error::new(io::ErrorKind::ConnectionRefused, UNANSWERED));
            }
            Err(error) => return Err(error),
        }
    }
    let reply = String::from_utf8_lossy(&reply);
    match reply.strip_prefix(ACCEPTED).map(str::parse::<u32>) {
        Some(Ok(_host_port)) => Ok(stream),
        _refused => Err(io::Error::new(
            io::ErrorKind::ConnectionRefused,
            format!("the guest's monitor answered {reply:?}"),
        )),
    }
}

/// Whether `failure` is a guest still booting: its monitor's socket not made
/// yet, or no guest process listening yet.
fn booting(failure: &io::Error) -> bool {
    matches!(
        failure.kind(),
        io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused | io::ErrorKind::ConnectionReset
    )
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
    )]

    use std::time::Duration;

    use afr_executor::Transport as _;
    use tokio::io::{AsyncBufReadExt as _, AsyncReadExt as _, AsyncWriteExt as _, BufReader};
    use tokio::net::UnixListener;

    use super::VsockTransport;

    #[tokio::test]
    async fn test_a_dial_crosses_the_connect_handshake_and_refuses_an_unheard_port() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("v.sock");
        let monitor = UnixListener::bind(&socket).unwrap();
        tokio::spawn(async move {
            while let Ok((stream, _peer)) = monitor.accept().await {
                let mut stream = BufReader::new(stream);
                let mut asked = String::new();
                stream.read_line(&mut asked).await.unwrap();
                if asked == "CONNECT 1024\n" {
                    stream.write_all(b"OK 1073741824\nlisted").await.unwrap();
                }
            }
        });
        let within = Duration::from_millis(100);

        let mut served = VsockTransport::new(socket.clone(), 1024, within)
            .dial()
            .await
            .unwrap();
        let unheard = VsockTransport::new(socket, 1025, within)
            .dial()
            .await
            .err()
            .unwrap();

        let mut said = [0; 6];
        served.read_exact(&mut said).await.unwrap();
        assert_eq!(&said, b"listed");
        assert_eq!(unheard.kind(), std::io::ErrorKind::ConnectionRefused);
    }
}
