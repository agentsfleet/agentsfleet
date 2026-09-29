//! A Dragonfly that answers wrongly, on purpose.
//!
//! Several branches in this crate exist for a server that misbehaves, and a
//! real Dragonfly never does: a `PING` answered with something that is not `PONG`,
//! an `XADD` answered with an empty id, a socket that accepts a command and
//! then dies, and a pub/sub connection that comes back up but refuses the
//! resubscribe. The live-service suite cannot reach any of them, because the
//! service it points at is correct. Pointing at a server that is deliberately
//! not correct is the only honest way in.
//!
//! Plain TCP, never TLS. The transport is not what is under test here — the
//! reply shape and the socket's lifetime are — and terminating TLS in a test
//! server would add a certificate to maintain for no claim it would let us
//! make.
//!
//! Only the sliver of RESP the client actually speaks is parsed: a request is
//! an array of bulk strings, the first of which is the command name. Replies go
//! out as raw bytes the test chooses, which is the point — a reply builder that
//! only produced well-formed answers could not produce these.

#![allow(
    dead_code,
    reason = "support module: `#[path]`-included into several test crates, each of which drives \
              a different subset of the fault surface — the unused half in any one of them is \
              used in another, and per-crate cfg would be worse than saying so once"
)]

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use tokio::net::TcpListener;

pub(crate) use crate::subscriber::install_subscriber;

/// A frame the server writes unprompted.
#[derive(Debug, Clone)]
struct Push {
    /// The channel it was published on, or `None` for a push every
    /// connection is sent.
    channel: Option<Vec<u8>>,
    frame: Vec<u8>,
}

/// Shared state the test drives the server through mid-flight.
#[derive(Debug)]
struct Control {
    /// The port this server listens on, which the cluster topology it
    /// advertises has to name.
    port: u16,
    /// The rule table, mutable mid-flight: a test makes the FIRST subscribe
    /// succeed and a later one fail, which is the only way to reach a redial
    /// that connects and then cannot resubscribe.
    rules: Mutex<HashMap<String, Reply>>,
    /// Every command the server has parsed, in arrival order, so a test can
    /// assert on what the client actually sent rather than assuming.
    seen: Mutex<Vec<String>>,
    /// Signals live connections to drop. A broadcast because there may be
    /// several and every one of them has to hear it.
    cut: tokio::sync::broadcast::Sender<()>,
    /// Bytes live connections write unprompted: a published frame, written
    /// only where its channel is subscribed, or a push for every connection.
    pushes: tokio::sync::broadcast::Sender<Push>,
    /// Connections currently being served. Counted server-side because it is
    /// the only place that can tell a client which CLOSED its socket from one
    /// that merely stopped using it.
    live: Arc<std::sync::atomic::AtomicUsize>,
}

/// A server that answers a fixed reply per command name.
///
/// Commands with no rule get `+OK`, which is what keeps the client's own
/// connection setup (`CLIENT SETINFO`, and anything a future version adds)
/// working without every test having to know about it.
#[derive(Debug)]
pub(crate) struct FakeRedis {
    addr: SocketAddr,
    control: Arc<Control>,
    listening: tokio::sync::watch::Sender<bool>,
}

impl FakeRedis {
    /// Binds an ephemeral port and serves `rules` until dropped.
    ///
    /// Rule keys are matched upper-case, because the client is free to send
    /// either spelling and does not promise which.
    pub(crate) async fn spawn(rules: &[(&str, Reply)]) -> Self {
        // Port 0: the kernel picks, so parallel tests never contend for a
        // number and no test has to reserve one.
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("the fake server must be able to bind a loopback port");
        let addr = listener
            .local_addr()
            .expect("a bound listener has an address");

        let (cut, _first) = tokio::sync::broadcast::channel(16);
        let (pushes, _none_yet) = tokio::sync::broadcast::channel(64);
        let control = Arc::new(Control {
            port: addr.port(),
            rules: Mutex::new(rule_table(rules)),
            seen: Mutex::new(Vec::new()),
            cut,
            pushes,
            live: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        });
        let (listening, stopped) = tokio::sync::watch::channel(true);
        tokio::spawn(accept(listener, Arc::clone(&control), stopped));

        Self {
            addr,
            control,
            listening,
        }
    }

    /// The URL a client connects to this fake with.
    pub(crate) fn url(&self) -> String {
        format!("redis://{}", self.addr)
    }

    /// Changes the answer to one command, from the next one onward.
    ///
    /// Mid-flight rather than at construction because the interesting states
    /// are transitions: a server that answered `SUBSCRIBE` and then stopped is
    /// a failover, and a fixture fixed at spawn time could only describe the
    /// before or the after, never the change between them.
    pub(crate) fn set_reply(&self, command: &str, reply: Reply) {
        self.control
            .rules
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(command.to_uppercase(), reply);
    }

    /// Drops every live connection, leaving the listener up.
    ///
    /// This is the socket dying underneath a client that is still running —
    /// an idle timeout, a failover, a `CLIENT KILL`. What it is NOT is the
    /// server going away, which is [`FakeRedis::stop_listening`].
    pub(crate) fn cut(&self) {
        let _delivered = self.control.cut.send(());
    }

    /// Frees the port, so anything that redials is refused.
    ///
    /// Deliberately separate from `cut`: a reconnect that is refused and a
    /// reconnect that succeeds onto a broken socket are different paths through
    /// the pump, and a test that could only produce one of them would leave the
    /// other unproven.
    pub(crate) fn stop_listening(&self) {
        let _delivered = self.listening.send(false);
    }

    /// Publishes `payload` on `channel` to every live connection, as the
    /// `smessage` push a subscribed client receives. Unconditional: the fake
    /// keeps no subscription table, so a client that never subscribed is sent
    /// the frame too, and a test only asserts on the reader it subscribed.
    pub(crate) fn publish(&self, channel: &str, payload: &str) {
        let _delivered = self.control.pushes.send(Push {
            channel: Some(channel.as_bytes().to_vec()),
            frame: smessage(channel, payload),
        });
    }

    /// Writes `frame` to every live connection unprompted — a push of any
    /// kind, well-formed or not, as the test builds it.
    pub(crate) fn push(&self, frame: Vec<u8>) {
        let _delivered = self.control.pushes.send(Push {
            channel: None,
            frame,
        });
    }

    /// How many connections the server is currently serving.
    pub(crate) fn live_connections(&self) -> usize {
        self.control.live.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Every command the server has parsed so far, in arrival order.
    pub(crate) fn seen(&self) -> Vec<String> {
        self.control
            .seen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

impl Drop for FakeRedis {
    fn drop(&mut self) {
        // The accept loop owns the listener, so telling it to stop is what
        // actually frees the port. A test that leaked one would still pass, and
        // the next run on a busy machine would be the one that failed.
        self.stop_listening();
        self.cut();
    }
}

/// `rules`, keyed upper-case, with the answers every cluster client needs
/// filled in where a test left them out.
fn rule_table(rules: &[(&str, Reply)]) -> HashMap<String, Reply> {
    let mut table: HashMap<String, Reply> = rules
        .iter()
        .map(|(name, reply)| ((*name).to_uppercase(), reply.clone()))
        .collect();
    // The handshake a cluster client performs before its first command:
    // a test that scripts one fault should not have to know it exists,
    // and one that wants to break it names `CLUSTER` itself.
    table
        .entry(CMD_CLUSTER.to_owned())
        .or_insert(Reply::ClusterSlots);
    // `INFO cluster` is how preflight asks what the server IS, and every
    // fake server in this workspace is a cluster unless a test says
    // otherwise. Its own key because the memory section shares the command.
    table
        .entry(RULE_INFO_CLUSTER.to_owned())
        .or_insert(Reply::InCluster);
    table
}

/// Accepts connections until told to stop listening, serving each on a task
/// of its own.
async fn accept(
    listener: TcpListener,
    control: Arc<Control>,
    mut stopped: tokio::sync::watch::Receiver<bool>,
) {
    loop {
        let accepted = tokio::select! {
            result = listener.accept() => result,
            _stop = stopped.changed() => return,
        };
        let Ok((socket, _peer)) = accepted else {
            return;
        };
        tokio::spawn(serve(socket, Arc::clone(&control)));
    }
}

#[path = "fake_redis/resp.rs"]
mod resp;

#[path = "fake_redis/reply.rs"]
mod reply;

#[path = "fake_redis/serve.rs"]
mod serve;

use self::reply::{CMD_CLUSTER, RULE_INFO_CLUSTER, smessage};
pub(crate) use self::reply::{Reply, push_frame};
use self::serve::serve;

/// A loopback port with nothing listening on it.
///
/// Bound and released, so the number is real and known-free rather than
/// guessed: a hard-coded port that something else on the machine happens to
/// hold would turn "connection refused" into a connection that succeeds.
pub(crate) async fn closed_port() -> String {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("binding a loopback port must succeed");
    let addr = listener
        .local_addr()
        .expect("a bound listener has an address");
    drop(listener);
    format!("redis://{addr}")
}
