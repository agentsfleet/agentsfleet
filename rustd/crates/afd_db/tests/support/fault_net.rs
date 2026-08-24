//! A Postgres that stops answering, on purpose.
//!
//! Two branches in the pool exist for a datastore that accepts a socket and
//! then says nothing: the handshake deadline, and an acquire that cannot open a
//! connection at all. Neither can be reached against a healthy server, and
//! neither can be reached by pointing at a closed port either — a refused
//! connection fails immediately, which is the case these are NOT about.
//!
//! What produces them is a server that is reachable at the TCP level and
//! unresponsive above it: a Postgres wedged on disk, a firewall dropping packets
//! after the handshake, a failover that has bound the port before it can serve.
//! This proxy is that, and it can become that mid-run, which is what lets one
//! test connect successfully and then lose the datastore underneath it.
//!
//! Plain TCP forwarding, so it is transparent to whatever the client and server
//! say to each other. The local lane runs Postgres with `sslmode=disable`, and
//! nothing here inspects a byte in either direction regardless.

#![allow(
    dead_code,
    reason = "support module: `#[path]`-included into more than one test crate, each of \
              which drives a different half of the fault surface"
)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::net::{TcpListener, TcpStream};

/// A TCP proxy in front of a real server, which can be told to stop relaying.
#[derive(Debug)]
pub(crate) struct FaultProxy {
    addr: SocketAddr,
    /// When set, accepted connections are held open and never relayed. Held
    /// rather than closed on purpose: closing would give the client a clean
    /// error, and the branch under test is the one where no answer ever comes.
    swallowing: Arc<AtomicBool>,
    listening: tokio::sync::watch::Sender<bool>,
}

impl FaultProxy {
    /// Stands a proxy in front of `target`, relaying by default.
    pub(crate) async fn to(target: SocketAddr) -> Self {
        Self::spawn(target, false).await
    }

    /// Stands a proxy that swallows from its first connection onward.
    ///
    /// For the case where the datastore is unresponsive before anything has
    /// ever connected to it — a boot against a wedged server.
    pub(crate) async fn swallowing(target: SocketAddr) -> Self {
        Self::spawn(target, true).await
    }

    async fn spawn(target: SocketAddr, swallow: bool) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("the proxy must be able to bind a loopback port");
        let addr = listener
            .local_addr()
            .expect("a bound listener has an address");
        let swallowing = Arc::new(AtomicBool::new(swallow));
        let (listening, mut stopped) = tokio::sync::watch::channel(true);

        let mode = Arc::clone(&swallowing);
        tokio::spawn(async move {
            loop {
                let accepted = tokio::select! {
                    result = listener.accept() => result,
                    _stop = stopped.changed() => return,
                };
                let Ok((client, _peer)) = accepted else {
                    return;
                };
                if mode.load(Ordering::Acquire) {
                    // Parked, deliberately leaked for the life of the test: the
                    // socket has to stay OPEN for the client to keep waiting on
                    // it, and dropping it here is the one thing that would turn
                    // this into a refusal.
                    tokio::spawn(async move {
                        let held = client;
                        std::future::pending::<()>().await;
                        drop(held);
                    });
                    continue;
                }
                tokio::spawn(relay(client, target));
            }
        });

        Self {
            addr,
            swallowing,
            listening,
        }
    }

    /// The address a client connects to the proxy on.
    pub(crate) const fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Stops relaying: from here, connections are accepted and never answered.
    ///
    /// Existing relays are left alone. The pool under test opens its
    /// connections lazily, so what matters is what the NEXT one meets.
    pub(crate) fn swallow(&self) {
        self.swallowing.store(true, Ordering::Release);
    }
}

impl Drop for FaultProxy {
    fn drop(&mut self) {
        // Frees the port. A leaked listener would still pass this test and fail
        // a later one on a busy machine.
        let _delivered = self.listening.send(false);
    }
}

/// Copies bytes both ways until either side closes.
///
/// The result is discarded because there is nothing to do about it: a proxy
/// whose upstream refused, or whose client hung up mid-stream, has said
/// everything it can say by closing, and the test asserts on what the code
/// under test made of that.
async fn relay(mut client: TcpStream, target: SocketAddr) {
    let Ok(mut upstream) = TcpStream::connect(target).await else {
        return;
    };
    let _copied = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
}

/// Installs a subscriber so event macros actually run.
///
/// The same trap the live harness carries: `tracing::warn!` checks whether its
/// callsite is enabled before evaluating its fields, so a failure path exercised
/// without a subscriber runs without the line that reports it. Output goes to a
/// sink — the point is that the fields run.
pub(crate) fn install_subscriber() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_writer(std::io::sink)
            .finish();
        let _ignored = tracing::subscriber::set_global_default(subscriber);
    });
}
