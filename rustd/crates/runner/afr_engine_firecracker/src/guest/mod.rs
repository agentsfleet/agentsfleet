//! The guest side: the runner binary as a microVM's first process.
//!
//! Booted as the guest kernel's `init`, the entry mounts the guest's file
//! systems — the toolbox drive's `/usr` and `/etc`, the workspace drive at
//! `/workspace` and `/tmp` — then serves the executor on vsock, the host's
//! only way in. Only the host may connect: a peer whose context identifier is
//! not the host's is dropped before it reads a byte, and the guest kernel has
//! no vsock loopback, so no guest process can dial its own executor.
//!
//! What the entry is told on its command line is one type, [`GuestEntry`], so
//! the engine that writes the kernel's command line and the entry that reads
//! it never disagree.

use std::ffi::OsString;
use std::io;

use afr_executor::Sizes;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "linux")]
pub use self::linux::serve_vm;

/// The context identifier every vsock connection from the host carries:
/// `VMADDR_CID_HOST`, fixed by the vsock address family.
pub const HOST_CID: u32 = 2;
/// The port the guest's executor listens on: the first vsock port a process
/// binds without `CAP_NET_BIND_SERVICE`, which the kernel demands below 1024.
pub const EXECUTOR_PORT: u32 = 1024;
/// The runner's sub-command a guest kernel starts as `init`.
pub const GUEST_SUBCOMMAND: &str = "guest";
/// The flag naming the size of the guest's `/dev/shm`.
const SHARED_MEMORY_FLAG: &str = "--shared-memory-bytes";
/// The flags naming the executor's read size and frame.
const MAX_READ_BYTES_FLAG: &str = "--max-read-bytes";
const MAX_FRAME_BYTES_FLAG: &str = "--max-frame-bytes";
/// The event a refused peer is logged under.
const EVENT_PEER_REFUSED: &str = "executor_peer_refused";

/// What the guest entry is told on the kernel's command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::Args)]
pub struct GuestEntry {
    /// What the guest's `/dev/shm` may hold, in bytes.
    #[arg(long)]
    pub shared_memory_bytes: u64,
    /// The most one read answers with.
    #[arg(long, default_value_t = Sizes::DEFAULT.max_read_bytes)]
    pub max_read_bytes: u64,
    /// The longest line the executor reads.
    #[arg(long, default_value_t = Sizes::DEFAULT.max_frame_bytes)]
    pub max_frame_bytes: usize,
}

impl GuestEntry {
    /// The entry whose `/dev/shm` holds `shared_memory_bytes`, its executor
    /// serving the read size and frame of `sizes`.
    #[must_use]
    pub const fn new(shared_memory_bytes: u64, sizes: Sizes) -> Self {
        Self {
            shared_memory_bytes,
            max_read_bytes: sizes.max_read_bytes,
            max_frame_bytes: sizes.max_frame_bytes,
        }
    }

    /// The sub-command and its flags, as the kernel hands them to `init`.
    #[must_use]
    pub fn arguments(self) -> Vec<OsString> {
        [
            GUEST_SUBCOMMAND.to_owned(),
            SHARED_MEMORY_FLAG.to_owned(),
            self.shared_memory_bytes.to_string(),
            MAX_READ_BYTES_FLAG.to_owned(),
            self.max_read_bytes.to_string(),
            MAX_FRAME_BYTES_FLAG.to_owned(),
            self.max_frame_bytes.to_string(),
        ]
        .into_iter()
        .map(OsString::from)
        .collect()
    }

    /// The sizes its executor serves with.
    #[must_use]
    pub const fn sizes(self) -> Sizes {
        Sizes {
            max_read_bytes: self.max_read_bytes,
            max_frame_bytes: self.max_frame_bytes,
            ..Sizes::DEFAULT
        }
    }
}

/// A listener the guest's executor accepts connections on, each with the
/// context identifier of its peer.
#[async_trait::async_trait]
pub trait Accept: Send {
    /// What one accepted connection is.
    type Stream: Send;

    /// The next connection and its peer's context identifier.
    ///
    /// # Errors
    /// The listener failed.
    async fn accept(&mut self) -> io::Result<(Self::Stream, u32)>;
}

/// The next connection from the host on `listener`; every connection from
/// any other peer is dropped unread and logged.
///
/// # Errors
/// The listener failed.
pub async fn host_connection<A: Accept>(listener: &mut A) -> io::Result<A::Stream> {
    loop {
        let (stream, peer) = listener.accept().await?;
        if peer == HOST_CID {
            return Ok(stream);
        }
        drop(stream);
        let event = EVENT_PEER_REFUSED;
        tracing::warn!(peer, event);
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
    )]

    use std::collections::VecDeque;
    use std::io;

    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _, DuplexStream};

    use super::{Accept, HOST_CID, host_connection};

    /// A listener answering the connections it was given, in order, each
    /// with the peer identifier it was given.
    struct Fake(VecDeque<(DuplexStream, u32)>);

    #[async_trait::async_trait]
    impl Accept for Fake {
        type Stream = DuplexStream;

        async fn accept(&mut self) -> io::Result<(DuplexStream, u32)> {
            self.0
                .pop_front()
                .ok_or_else(|| io::ErrorKind::ConnectionAborted.into())
        }
    }

    #[tokio::test]
    async fn test_the_executor_refuses_a_peer_that_is_not_the_host() {
        let (guest_a, mut guest_peer) = tokio::io::duplex(64);
        let (loopback, mut loopback_peer) = tokio::io::duplex(64);
        let (host, mut host_peer) = tokio::io::duplex(64);
        let mut listener = Fake(VecDeque::from([
            (guest_a, 3),
            (loopback, 1),
            (host, HOST_CID),
        ]));

        let mut served = host_connection(&mut listener).await.unwrap();

        for refused in [&mut guest_peer, &mut loopback_peer] {
            let mut read = Vec::new();
            refused.read_to_end(&mut read).await.unwrap();
            assert!(read.is_empty(), "a refused peer is dropped unread");
        }
        host_peer.write_all(b"list").await.unwrap();
        let mut heard = [0; 4];
        served.read_exact(&mut heard).await.unwrap();
        assert_eq!(&heard, b"list");
        let none = host_connection(&mut Fake(VecDeque::from([(served, 3)])))
            .await
            .unwrap_err();
        assert_eq!(none.kind(), io::ErrorKind::ConnectionAborted);
    }
}
