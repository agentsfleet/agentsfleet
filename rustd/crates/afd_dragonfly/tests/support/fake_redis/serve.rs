//! One connection to the fake, served until it closes, is cut, or a rule
//! says to hang up.
//!
//! Split from `fake_redis.rs` so the server's setup and one connection's
//! loop each fit the length caps. The write half is shared, because two
//! things write besides the reply loop: a held acknowledgement that lands
//! later, and a published frame that lands whenever the test says.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpStream;
use tokio::net::tcp::OwnedWriteHalf;

use super::Control;
use super::reply::{
    INFO_CLUSTER_DISABLED, INFO_CLUSTER_ENABLED, Reply, bulk, cluster_topology, confirmation,
    rule_key,
};
use super::resp::{Request, parse_command};

/// The write half every writer on one connection shares.
type Writer = Arc<tokio::sync::Mutex<OwnedWriteHalf>>;

/// What one request is answered with.
enum Answer {
    /// These bytes, now.
    Write(Vec<u8>),
    /// Nothing now: never, or later from a task of its own.
    Nothing,
    /// Close the connection.
    Hangup,
}

/// Answers one connection until it closes, is cut, or a rule says to hang up.
pub(super) async fn serve(socket: TcpStream, control: Arc<Control>) {
    control.live.fetch_add(1, Ordering::AcqRel);
    // The decrement rides a guard so it happens on EVERY exit from this
    // function, including the early returns a hangup rule takes.
    let _open = OpenConnection(Arc::clone(&control.live));
    let mut cut = control.cut.subscribe();
    let mut pushes = control.pushes.subscribe();
    let (mut socket, writer) = socket.into_split();
    let writer: Writer = Arc::new(tokio::sync::Mutex::new(writer));
    let mut buffer = Vec::new();
    let mut scratch = [0_u8; 4096];

    loop {
        if !answer_buffered(&mut buffer, &control, &writer).await {
            return;
        }
        let read = tokio::select! {
            result = socket.read(&mut scratch) => result,
            _cut = cut.recv() => return,
            published = pushes.recv() => {
                if let Ok(bytes) = published
                    && writer.lock().await.write_all(&bytes).await.is_err()
                {
                    return;
                }
                continue;
            }
        };
        match read {
            Ok(0) | Err(_) => return,
            Ok(count) => buffer.extend_from_slice(scratch.get(..count).unwrap_or_default()),
        }
    }
}

/// Answers every request already buffered. `false` means the connection ends.
///
/// Everything buffered is parsed before asking for more: one read can carry
/// several pipelined commands, and a server that answered only the first
/// would hang the client waiting for the rest.
async fn answer_buffered(buffer: &mut Vec<u8>, control: &Control, writer: &Writer) -> bool {
    while let Some(request) = parse_command(buffer) {
        buffer.drain(..request.consumed);
        control
            .seen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(request.name.clone());
        let bytes = match answer(&request, control, writer) {
            Answer::Write(bytes) => bytes,
            Answer::Nothing => continue,
            Answer::Hangup => return false,
        };
        if writer.lock().await.write_all(&bytes).await.is_err() {
            return false;
        }
    }
    true
}

/// What the rule table says to answer `request` with.
fn answer(request: &Request, control: &Control, writer: &Writer) -> Answer {
    let reply = control
        .rules
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(rule_key(request).as_str())
        .cloned()
        .unwrap_or(Reply::Raw("+OK\r\n"));
    let argument = request.first_argument();
    Answer::Write(match reply {
        Reply::Raw(raw) => raw.as_bytes().to_vec(),
        Reply::Hangup => return Answer::Hangup,
        Reply::Silent => return Answer::Nothing,
        Reply::SubscribeAck => confirmation("ssubscribe", argument),
        Reply::UnsubscribeAck => confirmation("sunsubscribe", argument),
        Reply::HeldSubscribeAck(hold) => {
            let ack = confirmation("ssubscribe", argument);
            tokio::spawn(write_later(Arc::clone(writer), hold, ack));
            return Answer::Nothing;
        }
        Reply::Bulk(payload) => bulk(payload),
        Reply::ClusterSlots => cluster_topology(control.port, argument),
        Reply::InCluster => bulk(INFO_CLUSTER_ENABLED),
        Reply::NotACluster => bulk(INFO_CLUSTER_DISABLED),
    })
}

/// Writes `bytes` after `hold`, leaving the connection serving meanwhile.
async fn write_later(writer: Writer, hold: Duration, bytes: Vec<u8>) {
    tokio::time::sleep(hold).await;
    let _gone = writer.lock().await.write_all(&bytes).await;
}

/// Decrements the live-connection count when a connection is done.
#[derive(Debug)]
struct OpenConnection(Arc<AtomicUsize>);

impl Drop for OpenConnection {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}
