//! A loopback scheduler whose every answer the test decides, and decides WHEN.
//!
//! The lane's real scheduler answers the way a healthy vendor does, which
//! leaves two things unreachable through it: a vendor refusing a removal, and
//! a removal that is in flight while something else happens to the row. Here
//! each request is reported as it arrives and held until the test releases it
//! with the status to answer, so "while the scheduler is being asked" is a
//! point the test can stand at rather than a race it hopes to win.

#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]
// `allow`, not `expect`, for the reason `cron_lane.rs` gives: each test binary
// that includes this module uses a different subset of it.
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]

use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::TcpListener;
use std::sync::{Arc, Mutex, mpsc};

use afd_cron::qstash::QStash;
use afd_crypto::secret::SecretString;
use tokio::sync::mpsc as async_mpsc;

/// Where a fire would come back; never dialled, since nothing here fires.
const DESTINATION: &str = "https://api.example.com/v1/schedules/fire";

/// One scheduler on a loopback port, serving requests one at a time.
#[derive(Debug)]
pub(crate) struct StubScheduler {
    base: String,
    arrived: async_mpsc::UnboundedReceiver<String>,
    release: mpsc::Sender<(u16, String)>,
    /// The `upstash-schedule-id` each request carried, in arrival order.
    schedule_ids: Arc<Mutex<Vec<Option<String>>>>,
}

/// The status [`StubScheduler::answer_lost`] queues: read the request, then
/// close the connection without a reply.
const LOST: u16 = 0;

impl StubScheduler {
    /// Starts listening; each request waits for [`Self::answer`].
    pub(crate) fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
        let base = format!(
            "http://{}",
            listener.local_addr().expect("the bound address")
        );
        let (arrive, arrived) = async_mpsc::unbounded_channel();
        let (release, released) = mpsc::channel::<(u16, String)>();
        let schedule_ids = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&schedule_ids);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut reader = BufReader::new(stream.try_clone().expect("clone the socket"));
                let mut request_line = String::new();
                if reader.read_line(&mut request_line).is_err() {
                    return;
                }
                // The head ends at the first blank line. A push carries a body,
                // and it is read off the socket before the answer: closing on
                // unread bytes resets the connection, and the client can then
                // lose the answer it was owed.
                let mut length = 0;
                let mut schedule_id = None;
                let mut header = String::new();
                while reader.read_line(&mut header).is_ok_and(|read| read > 2) {
                    if let Some((name, value)) = header.split_once(':') {
                        if name.eq_ignore_ascii_case("content-length") {
                            length = value.trim().parse().expect("a numeric content-length");
                        } else if name.eq_ignore_ascii_case("upstash-schedule-id") {
                            schedule_id = Some(value.trim().to_owned());
                        }
                    }
                    header.clear();
                }
                seen.lock().expect("the id log").push(schedule_id);
                let mut body = vec![0; length];
                if reader.read_exact(&mut body).is_err() {
                    return;
                }
                if arrive.send(request_line.trim_end().to_owned()).is_err() {
                    return;
                }
                let Ok((status, body)) = released.recv() else {
                    return;
                };
                if status == LOST {
                    drop(stream);
                    continue;
                }
                let answer = format!(
                    "HTTP/1.1 {status} STUB\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _sent = stream.write_all(answer.as_bytes());
            }
        });
        Self {
            base,
            arrived,
            release,
            schedule_ids,
        }
    }

    /// A client for this scheduler, as the boot path builds one.
    pub(crate) fn client(&self) -> QStash {
        client_at(self.base.clone())
    }

    /// The next request's line (`METHOD /path HTTP/1.1`), once it has arrived.
    pub(crate) async fn arrived(&mut self) -> String {
        self.arrived
            .recv()
            .await
            .expect("the scheduler thread is listening")
    }

    /// Every request line that has arrived and not been read, without waiting.
    ///
    /// Read after a call returns: each request is reported before it is
    /// answered, so a client that has its last answer has been fully seen. A
    /// missing repeat then fails the count instead of hanging the test.
    pub(crate) fn drained(&mut self) -> Vec<String> {
        std::iter::from_fn(|| self.arrived.try_recv().ok()).collect()
    }

    /// Lets the oldest waiting request go, answered with `status`. Sent ahead
    /// of a request, it answers that request the moment it arrives.
    pub(crate) fn answer(&self, status: u16) {
        self.answer_with(status, "");
    }

    /// Reads the next request and closes the connection without answering it:
    /// the scheduler may have filed the push, and the client never learns.
    pub(crate) fn answer_lost(&self) {
        self.answer_with(LOST, "");
    }

    /// The `upstash-schedule-id` header of every request so far, in order.
    pub(crate) fn schedule_ids(&self) -> Vec<Option<String>> {
        self.schedule_ids.lock().expect("the id log").clone()
    }

    /// As [`Self::answer`], with `body` as the answer's JSON.
    pub(crate) fn answer_with(&self, status: u16, body: &str) {
        self.release
            .send((status, body.to_owned()))
            .expect("the scheduler thread is listening");
    }
}

/// A client for loopback port 1, where nothing listens, so every call it
/// makes is refused before a request is sent. A privileged port, so no test
/// can bind it: a parallel stub can never be handed it and answer the call.
pub(crate) fn unreachable_client() -> QStash {
    client_at("http://127.0.0.1:1".to_owned())
}

/// A client for the scheduler at `base`, as the boot path builds one.
#[expect(
    clippy::disallowed_methods,
    reason = "the stub listens on loopback; no environment proxy is in play"
)]
pub(crate) fn client_at(base: String) -> QStash {
    QStash::new(
        reqwest::Client::new(),
        SecretString::new("stub-token".to_owned()),
        DESTINATION.to_owned(),
        base,
    )
}
