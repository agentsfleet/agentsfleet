//! The agent as a microVM's first process: what it mounts, and the only
//! connection it serves.
//!
//! Booted as the guest kernel's `init`, the agent mounts the guest's file
//! systems — the toolbox drive's `/usr` and `/etc`, the workspace drive at
//! `/workspace` and `/tmp` — then serves on vsock, the host's only way in.
//! Only the host may connect: a peer whose context identifier is not the
//! host's is dropped before it reads a byte, and the guest kernel has no
//! vsock loopback, so no guest process can dial its own agent.

use std::io;

use af_guest_wire::HOST_CID;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
mod loopback;

#[cfg(target_os = "linux")]
pub(crate) use self::linux::serve_vm;

/// The event a refused peer is logged under.
#[cfg_attr(
    not(any(target_os = "linux", test)),
    expect(dead_code, reason = "only a Linux guest listens on vsock")
)]
const EVENT_PEER_REFUSED: &str = "agent_peer_refused";

/// A listener the guest agent accepts connections on, each with the
/// context identifier of its peer.
#[async_trait::async_trait]
#[cfg_attr(
    not(any(target_os = "linux", test)),
    expect(dead_code, reason = "only a Linux guest listens on vsock")
)]
pub(crate) trait PeerConnections: Send {
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
#[cfg_attr(
    not(any(target_os = "linux", test)),
    expect(dead_code, reason = "only a Linux guest listens on vsock")
)]
pub(crate) async fn host_connection<A: PeerConnections>(listener: &mut A) -> io::Result<A::Stream> {
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
        reason = "a test asserts by panicking; the manifest's restriction set is for the agent"
    )]

    use std::collections::VecDeque;
    use std::io;

    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _, DuplexStream};

    use super::{HOST_CID, PeerConnections, host_connection};

    /// A listener answering the connections it was given, in order, each
    /// with the peer identifier it was given.
    struct Fake(VecDeque<(DuplexStream, u32)>);

    #[async_trait::async_trait]
    impl PeerConnections for Fake {
        type Stream = DuplexStream;

        async fn accept(&mut self) -> io::Result<(DuplexStream, u32)> {
            self.0
                .pop_front()
                .ok_or_else(|| io::ErrorKind::ConnectionAborted.into())
        }
    }

    #[tokio::test]
    async fn test_the_agent_refuses_a_peer_that_is_not_the_host() {
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
