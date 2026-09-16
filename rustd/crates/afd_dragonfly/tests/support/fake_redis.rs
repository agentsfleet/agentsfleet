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

use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};

/// The topology question every cluster client asks first.
const CMD_CLUSTER: &str = "CLUSTER";

pub(crate) use crate::subscriber::install_subscriber;

/// What the fake does when a command arrives.
#[derive(Debug, Clone)]
pub(crate) enum Reply {
    /// Write these bytes back. RESP, well-formed or not, as the test chooses.
    Raw(&'static str),
    /// Answer nothing and close the socket. This is a server dying mid-command,
    /// which is what the dropped-connection classification is written for.
    Hangup,
    /// Keep the socket open and never answer this command.
    Silent,
    /// The confirmation a server sends for `SSUBSCRIBE`: a RESP3 push echoing
    /// the channel the client asked for. Built here rather than written
    /// literally because the channel name is the test's, not this file's, and
    /// a push rather than an array because that is what the driver matches to
    /// the command it is waiting on.
    SubscribeAck,
    /// The confirmation for `SUNSUBSCRIBE`, same reasoning.
    UnsubscribeAck,
    /// The answer to `CLUSTER SLOTS` a cluster client insists on before it
    /// sends anything else: this server owns every slot, at its own port. An
    /// empty hostname tells the driver to keep dialling the address it came
    /// in on. Installed by default so a test scripting one fault does not
    /// have to know the handshake. The same rule answers `CLUSTER SHARDS`,
    /// which is how this crate's per-node walks find the one node to visit.
    /// A bulk string, framed from its payload.
    ///
    /// Its own variant rather than a `Raw` literal carrying its own `$NN`,
    /// because both `INFO` fixtures in this workspace carried one that
    /// disagreed with the bytes after it — `$52` over 57, `$41` over 45. A
    /// short length leaves the client parsing the remainder as the next
    /// reply and the test hanging on a deadline, which reads like a slow
    /// datastore rather than a miscounted fixture. Nothing here counts
    /// bytes by hand any more.
    Bulk(&'static str),
    ClusterSlots,
    /// `INFO cluster` as a server in cluster mode answers it.
    ///
    /// Installed by default on the synthetic `INFO CLUSTER` rule key, so a
    /// test scripting an `INFO` fault for the memory section does not also
    /// answer the cluster probe with memory text.
    InCluster,
    /// `INFO cluster` as a standalone answers it — the seed preflight refuses.
    ///
    /// Keyed on `INFO CLUSTER` and not on `INFO`, because preflight asks two
    /// sections of the same command and the driver's own handshake asks a
    /// third thing entirely; one rule for the whole command cannot tell them
    /// apart.
    NotACluster,
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

        // Port 0: the kernel picks, so parallel tests never contend for a
        // number and no test has to reserve one.
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("the fake server must be able to bind a loopback port");
        let addr = listener
            .local_addr()
            .expect("a bound listener has an address");

        let (cut, _first) = tokio::sync::broadcast::channel(16);
        let control = Arc::new(Control {
            port: addr.port(),
            rules: Mutex::new(table),
            seen: Mutex::new(Vec::new()),
            cut,
            live: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        });
        let (listening, mut stopped) = tokio::sync::watch::channel(true);

        let accepting = Arc::clone(&control);
        tokio::spawn(async move {
            loop {
                let accepted = tokio::select! {
                    result = listener.accept() => result,
                    _stop = stopped.changed() => return,
                };
                let Ok((socket, _peer)) = accepted else {
                    return;
                };
                tokio::spawn(serve(socket, Arc::clone(&accepting)));
            }
        });

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

/// Answers one connection until it closes, is cut, or a rule says to hang up.
async fn serve(mut socket: TcpStream, control: Arc<Control>) {
    control
        .live
        .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
    // The decrement rides a guard so it happens on EVERY exit from this
    // function, including the early returns a hangup rule takes.
    let _open = OpenConnection(Arc::clone(&control.live));
    let mut cut = control.cut.subscribe();
    let mut buffer = Vec::new();
    let mut scratch = [0_u8; 4096];

    loop {
        // Parse everything already buffered before asking for more: one read
        // can carry several pipelined commands, and a server that answered only
        // the first would hang the client waiting for the rest.
        while let Some(request) = parse_command(&buffer) {
            buffer.drain(..request.consumed);
            control
                .seen
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(request.name.clone());

            let reply = control
                .rules
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get(rule_key(&request).as_str())
                .cloned()
                .unwrap_or(Reply::Raw("+OK\r\n"));
            let bytes = match reply {
                Reply::Raw(raw) => raw.as_bytes().to_vec(),
                Reply::Hangup => return,
                Reply::Silent => continue,
                Reply::SubscribeAck => confirmation("ssubscribe", request.first_argument()),
                Reply::UnsubscribeAck => confirmation("sunsubscribe", request.first_argument()),
                Reply::Bulk(payload) => bulk(payload),
                Reply::ClusterSlots => cluster_topology(control.port, request.first_argument()),
                Reply::InCluster => bulk(INFO_CLUSTER_ENABLED),
                Reply::NotACluster => bulk(INFO_CLUSTER_DISABLED),
            };
            if socket.write_all(&bytes).await.is_err() {
                return;
            }
        }

        let read = tokio::select! {
            result = socket.read(&mut scratch) => result,
            _cut = cut.recv() => return,
        };
        match read {
            Ok(0) | Err(_) => return,
            Ok(count) => buffer.extend_from_slice(scratch.get(..count).unwrap_or_default()),
        }
    }
}

/// Decrements the live-connection count when a connection is done.
#[derive(Debug)]
struct OpenConnection(Arc<std::sync::atomic::AtomicUsize>);

impl Drop for OpenConnection {
    fn drop(&mut self) {
        self.0.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
    }
}

/// Builds the `subscribe`/`unsubscribe` confirmation Dragonfly pushes back.
///
/// The trailing count is the number of channels the connection now holds. It is
/// reported as one because nothing in these tests branches on it, and a fixture
/// that tracked it would be modelling server state this file does not have.
fn confirmation(kind: &str, channel: &[u8]) -> Vec<u8> {
    // `>` is RESP3's push marker: the driver routes it to the push receiver
    // AND treats it as the reply to the subscribe it is waiting on.
    let mut out = format!(">3\r\n${}\r\n{kind}\r\n${}\r\n", kind.len(), channel.len()).into_bytes();
    out.extend_from_slice(channel);
    out.extend_from_slice(b"\r\n:1\r\n");
    out
}

/// The subcommand of `CLUSTER` that asks for the shard map.
const CLUSTER_SHARDS: &[u8] = b"SHARDS";

/// The command whose sections preflight reads, the section that says what the
/// server IS, and the synthetic rule key the two make together.
const CMD_INFO: &str = "INFO";
const SECTION_CLUSTER: &[u8] = b"CLUSTER";
const RULE_INFO_CLUSTER: &str = "INFO CLUSTER";

/// `INFO cluster` as a server in cluster mode answers it, and as one that is
/// not. Only the field preflight reads is carried, with the header a real
/// section leads with: the rest is a dozen counters no caller here looks at.
///
/// The section is `INFO cluster` and NOT `CLUSTER INFO` — they are different
/// replies, and Dragonfly v1.40.2 names `cluster_enabled` in only this one. A
/// fake that answered the other spelling is what let preflight ship reading a
/// field the real server never puts there.
const INFO_CLUSTER_ENABLED: &str = "# Cluster\r\ncluster_enabled:1\r\n";
const INFO_CLUSTER_DISABLED: &str = "# Cluster\r\ncluster_enabled:0\r\n";

/// The rule table key one request looks up.
///
/// Every command is keyed by its name, except `INFO`, whose section decides
/// which question is being asked: preflight reads `cluster` and `memory` off
/// the same command and a single rule could not answer both.
fn rule_key(request: &self::resp::Request) -> String {
    if request.name == CMD_INFO
        && request
            .first_argument()
            .eq_ignore_ascii_case(SECTION_CLUSTER)
    {
        return RULE_INFO_CLUSTER.to_owned();
    }
    request.name.clone()
}

/// Frames `payload` as a RESP bulk string, counting it rather than trusting
/// a number written beside it.
fn bulk(payload: &str) -> Vec<u8> {
    format!("${}\r\n{payload}\r\n", payload.len()).into_bytes()
}

/// One shard owning slots 0..=16383 at `port` — in the `SLOTS` framing the
/// driver handshakes with, or the `SHARDS` framing this crate walks nodes by.
fn cluster_topology(port: u16, subcommand: &[u8]) -> Vec<u8> {
    if subcommand.eq_ignore_ascii_case(CLUSTER_SHARDS) {
        return afd_dragonfly::test_util::cluster_shards_reply(port);
    }
    afd_dragonfly::test_util::cluster_slots_reply(port)
}

#[path = "fake_redis/resp.rs"]
mod resp;

use self::resp::parse_command;

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
