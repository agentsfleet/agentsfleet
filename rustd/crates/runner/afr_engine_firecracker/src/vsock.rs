//! How the host reaches a guest's executor: the monitor's vsock Unix socket,
//! then a `CONNECT <port>` line, which the monitor answers `OK <port>` once a
//! guest process listens on that port, and closes otherwise.
//!
//! A guest that is still booting has no listener yet, so a dial retries until
//! the guest answers or the boot's time is up. Each boot is a fresh guest,
//! and the executor client dials again after every one (`afr_executor`'s
//! [`Transport`]).
//!
//! The socket sits in the jail, which its monitor's user owns, and the root
//! runner dials it. So every dial opens the name beneath the jail with no
//! link followed, proves what it opened is a socket that jail's user owns,
//! and connects to that very inode: a monitor that swaps the name for a link
//! to another guest's socket is refused, never followed.

use std::io;
use std::os::fd::{AsRawFd as _, OwnedFd};
use std::path::{Path, PathBuf};
use std::time::Duration;

use afr_executor::transport::{Stream, Transport};
use backon::{ConstantBuilder, Retryable as _};
use rustix::fs::{FileType, Mode, OFlags, ResolveFlags};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::UnixStream;

/// What the monitor's answer starts with when a guest listens on the port.
const ACCEPTED: &str = "OK ";
/// The longest answer the monitor gives: `OK `, a 32-bit port in decimal and
/// its newline, as Firecracker's hybrid vsock writes it.
const REPLY_BYTES_MAX: usize = ACCEPTED.len() + 10 + 1;
/// The pause between two dials of a guest that is not listening yet.
const DIAL_RETRY_DELAY: Duration = Duration::from_millis(10);
/// Where the kernel names each of this process's descriptors.
const SELF_DESCRIPTORS: &str = "/proc/self/fd";
/// What a guest that closed the dial without an answer is reported as.
const UNANSWERED: &str = "the guest's monitor closed the connection without accepting it";
/// What a name in the jail that is no socket of its monitor's is refused as.
const NOT_ITS_SOCKET: &str = "the jail's vsock name is not a socket its monitor's user owns";
/// How the name is resolved: inside the jail, through no symbolic link and
/// no `/proc` magic link.
const BENEATH: ResolveFlags = ResolveFlags::BENEATH
    .union(ResolveFlags::NO_SYMLINKS)
    .union(ResolveFlags::NO_MAGICLINKS);

/// One guest's executor, through its monitor's vsock socket.
#[derive(Debug, Clone)]
pub struct VsockTransport {
    jail: PathBuf,
    socket: &'static str,
    owner: u32,
    port: u32,
    within: Duration,
}

impl VsockTransport {
    /// The executor on guest `port`, through the monitor socket named
    /// `socket` in the jail at `jail`, whose monitor runs as user `owner`;
    /// each dial retried until the guest answers or `within` has passed.
    #[must_use]
    pub const fn new(
        jail: PathBuf,
        socket: &'static str,
        owner: u32,
        port: u32,
        within: Duration,
    ) -> Self {
        Self {
            jail,
            socket,
            owner,
            port,
            within,
        }
    }

    /// One dial: the socket proved this jail's, then the handshake over it.
    async fn dial_once(&self) -> io::Result<UnixStream> {
        connect(&self.socket()?, self.port).await
    }

    /// The monitor's socket, opened as a path only: a descriptor on the
    /// inode itself, which no later swap of its name changes.
    ///
    /// # Errors
    /// The name is absent, is or crosses a link, leaves the jail, or names
    /// anything but a socket the jail's user owns.
    fn socket(&self) -> io::Result<OwnedFd> {
        let directory = OFlags::PATH | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
        let jail = rustix::fs::open(&self.jail, directory, Mode::empty())?;
        let named = OFlags::PATH | OFlags::CLOEXEC;
        let socket = rustix::fs::openat2(&jail, self.socket, named, Mode::empty(), BENEATH)?;
        let found = rustix::fs::fstat(&socket)?;
        if FileType::from_raw_mode(found.st_mode) == FileType::Socket && found.st_uid == self.owner
        {
            Ok(socket)
        } else {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                NOT_ITS_SOCKET,
            ))
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
        let stream = (|| self.dial_once()).retry(policy).when(booting).await?;
        Ok(Box::new(stream))
    }
}

/// Dials the monitor whose socket `socket` holds and asks it for guest
/// `port`. The dial names the descriptor, so it reaches the inode that was
/// checked, whatever its name in the jail says by now.
///
/// # Errors
/// The socket refuses, or the monitor answers anything but its acceptance.
async fn connect(socket: &OwnedFd, port: u32) -> io::Result<UnixStream> {
    let held = Path::new(SELF_DESCRIPTORS).join(socket.as_raw_fd().to_string());
    let mut stream = UnixStream::connect(held).await?;
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

    use std::io;
    use std::os::unix::fs::{MetadataExt as _, symlink};
    use std::time::Duration;

    use afr_executor::Transport as _;
    use tokio::io::{AsyncBufReadExt as _, AsyncReadExt as _, AsyncWriteExt as _, BufReader};
    use tokio::net::UnixListener;

    use super::VsockTransport;

    const SOCKET: &str = "v.sock";

    /// A monitor in `dir` under `name`, answering port 1024 alone.
    fn monitor(dir: &std::path::Path, name: &str) {
        let monitor = UnixListener::bind(dir.join(name)).unwrap();
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
    }

    #[tokio::test]
    async fn test_a_dial_crosses_the_connect_handshake_and_refuses_an_unheard_port() {
        let jail = tempfile::tempdir().unwrap();
        monitor(jail.path(), SOCKET);
        let owner = jail.path().metadata().unwrap().uid();
        let within = Duration::from_millis(100);
        let on = |port| VsockTransport::new(jail.path().to_owned(), SOCKET, owner, port, within);

        let mut served = on(1024).dial().await.unwrap();
        let unheard = on(1025).dial().await.err().unwrap();

        let mut said = [0; 6];
        served.read_exact(&mut said).await.unwrap();
        assert_eq!(&said, b"listed");
        assert_eq!(unheard.kind(), io::ErrorKind::ConnectionRefused);
    }

    /// A monitor that took over its jail swaps the socket's name for a link
    /// to another guest's socket, or for a socket another user owns: the
    /// root runner follows neither, and reaches no monitor but its own.
    #[tokio::test]
    async fn test_a_link_at_the_sockets_name_or_another_users_socket_is_never_dialled() {
        let jail = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        monitor(other.path(), SOCKET);
        symlink(other.path().join(SOCKET), jail.path().join(SOCKET)).unwrap();
        symlink(other.path(), jail.path().join("out")).unwrap();
        let owner = jail.path().metadata().unwrap().uid();
        let within = Duration::from_millis(100);
        let from = |jail: &std::path::Path, socket, owner| {
            VsockTransport::new(jail.to_owned(), socket, owner, 1024, within)
        };

        let linked = from(jail.path(), SOCKET, owner).dial().await.err().unwrap();
        let through = from(jail.path(), "out/v.sock", owner).dial().await;
        let foreign = from(other.path(), SOCKET, owner + 1).dial().await;

        let (through, foreign) = (through.err().unwrap(), foreign.err().unwrap());

        assert_eq!(linked.raw_os_error(), Some(libc::ELOOP), "{linked}");
        assert_eq!(through.raw_os_error(), Some(libc::ELOOP), "{through}");
        assert_eq!(foreign.kind(), io::ErrorKind::PermissionDenied);
        assert!(foreign.to_string().contains("not a socket its monitor"));
    }
}
