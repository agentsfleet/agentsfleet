//! A Redis that answers everything and then hangs up on the read.
//!
//! Purpose-built and deliberately tiny: the only behaviour under test is what
//! the worker does when its READ keeps failing, so this server needs to do
//! exactly three things — let a connection open, satisfy the client's own
//! setup, and refuse the read while counting how often it was asked. It is not
//! a Redis: `afd_dragonfly`'s own suites own the protocol-shaped fake, and a second
//! general one here would be a second thing to keep true.
//!
//! It does parse RESP arrays, because it has to. The client pipelines its
//! connection setup, and a server that answered one reply per READ rather than
//! one per COMMAND leaves the client waiting for the rest — which looks exactly
//! like an unreachable Redis and would make every test here pass for the wrong
//! reason.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};

/// The read this server refuses.
const CMD_XREADGROUP: &str = "XREADGROUP";

/// The topology command a cluster client sends before anything else.
const CMD_CLUSTER: &str = "CLUSTER";

/// The RESP terminator, and the two type bytes this parser reads.
const CRLF: &[u8] = b"\r\n";
const ARRAY: u8 = b'*';
const BULK: u8 = b'$';

const CMD_XACK: &str = "XACK";

/// One parsed command: what was asked, its last argument, and how many bytes
/// it took.
struct Request {
    name: String,
    last_argument: String,
    consumed: usize,
}

/// What the server does when it is asked to read.
///
/// The two are different faults and the client tells them apart: a hangup is an
/// error the driver reports at once and reconnects from, while a stall leaves
/// the socket open and answers nothing, so the client waits out its whole reply
/// deadline. A frozen host or a packet filter that drops rather than rejects
/// looks like the second.
#[derive(Debug, Clone, Copy)]
pub(crate) enum OnRead {
    HangUp,
    Stall,
}

/// A server that refuses every `XREADGROUP`, counting them, and accepts every
/// `XACK`, recording the id acknowledged.
///
/// The second half is what lets the lanes be graded without a live queue:
/// nothing in them reads, and the one thing they write is the ack.
#[derive(Debug)]
pub(crate) struct HangingQueue {
    addr: SocketAddr,
    reads: Arc<AtomicUsize>,
    acks: Arc<Mutex<Vec<String>>>,
}

impl HangingQueue {
    /// Binds a loopback port and serves until the test drops it.
    pub(crate) async fn spawn() -> Self {
        Self::spawn_with(OnRead::HangUp).await
    }

    /// Binds a server that accepts the read and then answers nothing.
    pub(crate) async fn spawn_stalling() -> Self {
        Self::spawn_with(OnRead::Stall).await
    }

    async fn spawn_with(on_read: OnRead) -> Self {
        // Port 0: the kernel picks, so parallel tests never contend.
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("the fake queue must be able to bind a loopback port");
        let addr = listener
            .local_addr()
            .expect("a bound listener has an address");
        let reads = Arc::new(AtomicUsize::new(0));
        let acks = Arc::new(Mutex::new(Vec::new()));

        let counting = Arc::clone(&reads);
        let recording = Arc::clone(&acks);
        let port = addr.port();
        tokio::spawn(async move {
            while let Ok((socket, _peer)) = listener.accept().await {
                tokio::spawn(serve(
                    socket,
                    Arc::clone(&counting),
                    Arc::clone(&recording),
                    on_read,
                    port,
                ));
            }
        });
        Self { addr, reads, acks }
    }

    /// The URL a client opens this server with.
    pub(crate) fn url(&self) -> String {
        format!("redis://{}/", self.addr)
    }

    /// How many reads the server has been asked for.
    pub(crate) fn reads(&self) -> usize {
        self.reads.load(Ordering::Acquire)
    }

    /// The ids acknowledged so far, in the order they arrived.
    pub(crate) fn acks(&self) -> Vec<String> {
        self.acks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

/// Answers one connection: the cluster handshake, `+PONG` to a ping, `+OK` to
/// anything else, and a hangup on the read.
///
/// `port` is this server's own, because the topology it advertises has to name
/// the address the client is already talking to.
async fn serve(
    mut socket: TcpStream,
    reads: Arc<AtomicUsize>,
    acks: Arc<Mutex<Vec<String>>>,
    on_read: OnRead,
    port: u16,
) {
    let mut buffer = Vec::new();
    let mut scratch = [0_u8; 4096];
    loop {
        // Everything already buffered is answered before more is asked for: one
        // read can carry several pipelined commands.
        while let Some(request) = parse(&buffer) {
            buffer.drain(..request.consumed);
            if request.name == CMD_XREADGROUP {
                reads.fetch_add(1, Ordering::AcqRel);
                match on_read {
                    OnRead::HangUp => return,
                    // Holds the connection open and never replies, so the client
                    // is left waiting on its own deadline rather than handed an
                    // error. Parking the task is what keeps the socket alive:
                    // returning would drop it, which IS the hangup case.
                    OnRead::Stall => std::future::pending::<()>().await,
                }
            }
            // The transport is cluster-only, so the driver asks for the
            // topology BEFORE it will ping. Answered from the shared builder
            // rather than respelled here — see `cluster_slots_reply`.
            let owned;
            let reply: &[u8] = if request.name == CMD_CLUSTER {
                owned = afd_dragonfly::test_util::cluster_slots_reply(port);
                &owned
            } else if request.name == "PING" {
                b"+PONG\r\n"
            } else if request.name == CMD_XACK {
                acks.lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(request.last_argument.clone());
                b":1\r\n"
            } else {
                b"+OK\r\n"
            };
            if socket.write_all(reply).await.is_err() {
                return;
            }
        }
        match socket.read(&mut scratch).await {
            Ok(0) | Err(_) => return,
            Ok(count) => buffer.extend_from_slice(scratch.get(..count).unwrap_or_default()),
        }
    }
}

/// The first whole command in `buffer`, if one is there.
///
/// Answers `None` for a partial command, which is the ordinary case on a
/// socket: the caller reads more and asks again.
fn parse(buffer: &[u8]) -> Option<Request> {
    let mut cursor = header(buffer, ARRAY)?;
    let arguments = cursor.count;
    let mut name = String::new();
    let mut last_argument = String::new();
    for index in 0..arguments {
        let bulk = header(buffer.get(cursor.end..)?, BULK)?;
        let start = cursor.end + bulk.end;
        let end = start + bulk.count;
        let argument = buffer.get(start..end)?;
        if index == 0 {
            name = String::from_utf8_lossy(argument).to_uppercase();
        }
        last_argument = String::from_utf8_lossy(argument).into_owned();
        cursor.end = end + CRLF.len();
        if buffer.len() < cursor.end {
            return None;
        }
    }
    Some(Request {
        name,
        last_argument,
        consumed: cursor.end,
    })
}

/// A RESP length header: the count it declares, and where it ends.
struct Header {
    count: usize,
    end: usize,
}

/// Reads a `*N\r\n` or `$N\r\n` header off the front of `buffer`.
fn header(buffer: &[u8], marker: u8) -> Option<Header> {
    if buffer.first() != Some(&marker) {
        return None;
    }
    let terminator = buffer.windows(CRLF.len()).position(|pair| pair == CRLF)?;
    let digits = std::str::from_utf8(buffer.get(1..terminator)?).ok()?;
    Some(Header {
        count: digits.parse().ok()?,
        end: terminator + CRLF.len(),
    })
}
