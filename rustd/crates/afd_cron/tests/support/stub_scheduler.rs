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

use std::io::{BufRead as _, BufReader, Write as _};
use std::net::TcpListener;
use std::sync::mpsc;

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
    release: mpsc::Sender<u16>,
}

impl StubScheduler {
    /// Starts listening; each request waits for [`Self::answer`].
    pub(crate) fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
        let base = format!(
            "http://{}",
            listener.local_addr().expect("the bound address")
        );
        let (arrive, arrived) = async_mpsc::unbounded_channel();
        let (release, released) = mpsc::channel::<u16>();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut reader = BufReader::new(stream.try_clone().expect("clone the socket"));
                let mut request_line = String::new();
                if reader.read_line(&mut request_line).is_err() {
                    return;
                }
                // The head ends at the first blank line; a removal has no body.
                let mut header = String::new();
                while reader.read_line(&mut header).is_ok_and(|read| read > 2) {
                    header.clear();
                }
                if arrive.send(request_line.trim_end().to_owned()).is_err() {
                    return;
                }
                let Ok(status) = released.recv() else { return };
                let answer = format!(
                    "HTTP/1.1 {status} STUB\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
                );
                let _sent = stream.write_all(answer.as_bytes());
            }
        });
        Self {
            base,
            arrived,
            release,
        }
    }

    /// A client for this scheduler, as the boot path builds one.
    #[expect(
        clippy::disallowed_methods,
        reason = "the stub listens on loopback; no environment proxy is in play"
    )]
    pub(crate) fn client(&self) -> QStash {
        QStash::new(
            reqwest::Client::new(),
            SecretString::new("stub-token".to_owned()),
            DESTINATION.to_owned(),
            self.base.clone(),
        )
    }

    /// The next request's line (`METHOD /path HTTP/1.1`), once it has arrived.
    pub(crate) async fn arrived(&mut self) -> String {
        self.arrived
            .recv()
            .await
            .expect("the scheduler thread is listening")
    }

    /// Lets the oldest waiting request go, answered with `status`. Sent ahead
    /// of a request, it answers that request the moment it arrives.
    pub(crate) fn answer(&self, status: u16) {
        self.release
            .send(status)
            .expect("the scheduler thread is listening");
    }
}
