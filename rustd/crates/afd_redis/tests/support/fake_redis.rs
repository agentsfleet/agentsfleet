//! A Redis that answers wrongly, on purpose.
//!
//! Several branches in this crate exist for a server that misbehaves, and a
//! real Redis never does: a `PING` answered with something that is not `PONG`,
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

/// What the fake does when a command arrives.
#[derive(Debug, Clone)]
pub(crate) enum Reply {
    /// Write these bytes back. RESP, well-formed or not, as the test chooses.
    Raw(&'static str),
    /// Answer nothing and close the socket. This is a server dying mid-command,
    /// which is what the dropped-connection classification is written for.
    Hangup,
    /// The confirmation Redis sends for `SUBSCRIBE`, echoing the channel the
    /// client asked for. Built here rather than written literally because the
    /// channel name is the test's, not this file's.
    SubscribeAck,
    /// The confirmation for `UNSUBSCRIBE`, same reasoning.
    UnsubscribeAck,
}

/// Shared state the test drives the server through mid-flight.
#[derive(Debug)]
struct Control {
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
        let table: HashMap<String, Reply> = rules
            .iter()
            .map(|(name, reply)| ((*name).to_uppercase(), reply.clone()))
            .collect();

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
            rules: Mutex::new(table),
            seen: Mutex::new(Vec::new()),
            cut,
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
                .get(&request.name)
                .cloned()
                .unwrap_or(Reply::Raw("+OK\r\n"));
            let bytes = match reply {
                Reply::Raw(raw) => raw.as_bytes().to_vec(),
                Reply::Hangup => return,
                Reply::SubscribeAck => confirmation("subscribe", request.first_argument()),
                Reply::UnsubscribeAck => confirmation("unsubscribe", request.first_argument()),
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

/// Builds the `subscribe`/`unsubscribe` confirmation Redis pushes back.
///
/// The trailing count is the number of channels the connection now holds. It is
/// reported as one because nothing in these tests branches on it, and a fixture
/// that tracked it would be modelling server state this file does not have.
fn confirmation(kind: &str, channel: &[u8]) -> Vec<u8> {
    let mut out = format!("*3\r\n${}\r\n{kind}\r\n${}\r\n", kind.len(), channel.len()).into_bytes();
    out.extend_from_slice(channel);
    out.extend_from_slice(b"\r\n:1\r\n");
    out
}

/// One parsed request.
#[derive(Debug)]
struct Request {
    name: String,
    arguments: Vec<Vec<u8>>,
    consumed: usize,
}

impl Request {
    /// The first argument, or empty when the command carried none.
    fn first_argument(&self) -> &[u8] {
        self.arguments.first().map_or(&[], Vec::as_slice)
    }
}

/// Reads one `*N` array of bulk strings.
///
/// `None` means the buffer holds an incomplete request and the caller should
/// read more — never that the request was bad. This server is a fixture, and a
/// fixture that tried to diagnose malformed input would be diagnosing the
/// client library rather than the code under test.
fn parse_command(buffer: &[u8]) -> Option<Request> {
    let mut cursor = 0_usize;
    let (header, used) = read_line(buffer, cursor)?;
    if !header.starts_with('*') {
        return None;
    }
    let count: usize = header.get(1..)?.parse().ok()?;
    cursor = used;

    let mut name = None;
    let mut arguments = Vec::new();
    for index in 0..count {
        let (marker, after_marker) = read_line(buffer, cursor)?;
        let length: usize = marker.strip_prefix('$')?.parse().ok()?;
        let end = after_marker.checked_add(length)?;
        // The trailing CRLF has to be present too, or the argument is only
        // partly here and the whole request must wait for another read.
        if buffer.get(end..end.checked_add(2)?)? != b"\r\n" {
            return None;
        }
        let field = buffer.get(after_marker..end)?;
        if index == 0 {
            name = Some(String::from_utf8_lossy(field).to_uppercase());
        } else {
            arguments.push(field.to_vec());
        }
        cursor = end.checked_add(2)?;
    }

    name.map(|command| Request {
        name: command,
        arguments,
        consumed: cursor,
    })
}

/// Reads one CRLF-terminated line, returning it and the offset past it.
fn read_line(buffer: &[u8], from: usize) -> Option<(String, usize)> {
    let rest = buffer.get(from..)?;
    let break_at = rest.windows(2).position(|pair| pair == b"\r\n")?;
    let line = String::from_utf8_lossy(rest.get(..break_at)?).into_owned();
    Some((line, from.checked_add(break_at)?.checked_add(2)?))
}

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

/// Installs a subscriber so event macros actually run.
///
/// The same trap the live harness carries, and it bites harder here: these
/// tests exist to exercise failure paths, and every one of those paths reports
/// itself through a `tracing` event whose fields are never evaluated without a
/// subscriber installed. Output goes to a sink — the point is that the fields
/// run, not that anybody reads them.
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
