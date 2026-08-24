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

use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
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

/// A byte sequence that, when the client sends it, kills the connection.
///
/// This is what makes "the connection died at exactly this statement" a
/// deterministic event rather than a race against a timer. The local lane runs
/// Postgres with `sslmode=disable`, so a query travels in plaintext and the
/// proxy can recognise the one it is waiting for.
type CutOn = Option<Arc<Vec<u8>>>;

impl FaultProxy {
    /// Stands a proxy in front of `target`, relaying by default.
    pub(crate) async fn to(target: SocketAddr) -> Self {
        Self::spawn(target, false, None).await
    }

    /// Stands a relaying proxy that kills a connection the moment its client
    /// sends `trigger`.
    ///
    /// For a branch that needs the datastore to die at ONE specific statement
    /// and nowhere earlier. Timing cannot express that — every statement before
    /// the interesting one runs on the same connection and would fail first —
    /// but the wire can, because the statement names itself.
    pub(crate) async fn cutting_on(target: SocketAddr, trigger: &[u8]) -> Self {
        Self::spawn(target, false, Some(Arc::new(trigger.to_vec()))).await
    }

    /// Stands a proxy that swallows from its first connection onward.
    ///
    /// For the case where the datastore is unresponsive before anything has
    /// ever connected to it — a boot against a wedged server.
    pub(crate) async fn swallowing(target: SocketAddr) -> Self {
        Self::spawn(target, true, None).await
    }

    async fn spawn(target: SocketAddr, swallow: bool, cut_on: CutOn) -> Self {
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
                tokio::spawn(relay(client, target, cut_on.clone()));
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
async fn relay(client: TcpStream, target: SocketAddr, cut_on: CutOn) {
    let Ok(upstream) = TcpStream::connect(target).await else {
        return;
    };
    let (mut from_client, mut to_client) = client.into_split();
    let (mut from_server, mut to_server) = upstream.into_split();

    // Replies need no inspection, so they stream on their own task. It ends
    // when the request side drops `to_server` and the server closes behind it,
    // which is what carries the cut back to the client as a dead socket.
    tokio::spawn(async move {
        let _copied = tokio::io::copy(&mut from_server, &mut to_client).await;
    });

    let mut buffer = [0_u8; 8192];
    loop {
        let Ok(read) = from_client.read(&mut buffer).await else {
            return;
        };
        let Some(chunk) = buffer.get(..read) else {
            return;
        };
        if chunk.is_empty() {
            return;
        }
        if cut_on
            .as_ref()
            .is_some_and(|trigger| contains(chunk, trigger))
        {
            // Dropping both halves here is the kill. The statement is never
            // forwarded, so the server never sees it and the client meets a
            // connection that died underneath the write.
            return;
        }
        if to_server.write_all(chunk).await.is_err() {
            return;
        }
    }
}

/// Whether `haystack` carries `needle` anywhere in it.
fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || needle.len() > haystack.len() {
        return false;
    }
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
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
