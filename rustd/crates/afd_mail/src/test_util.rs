//! Fixtures for the suites that send invite email: this crate's own and
//! `afd_api`'s integration lane. Compiled for `test` and under `test-util`.
//!
//! [`FakeRelay`] is a scripted SMTP relay on loopback, for the answers Mailpit
//! will not give. Mailpit accepts everything, which proves delivery and
//! nothing else. This relay plays one [`Session`] per connection, in order —
//! refuse at a stage, drop the connection after the message, stall, or accept
//! — and connections past the script are accepted. It records each message it
//! was given and counts connections, so a suite can assert what the retry and
//! the send-again carried and tell one try from a retry. One copy serves both
//! suites that drive lettre's real transport.

#![expect(
    clippy::expect_used,
    reason = "a test fixture whose precondition fails should stop the suite loudly"
)]

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};
use tokio::net::tcp::OwnedReadHalf;
use tokio::net::{TcpListener, TcpStream};

/// Where the relay listens, and the host its bag names.
pub const LOOPBACK: &str = "127.0.0.1";

/// The sender the relay's bag names.
pub const FROM: &str = "hello@agentsfleet.test";

/// The login the relay's bag names.
pub const USERNAME: &str = "relay";

/// The password the relay's bag names, which no record or `Debug` may print.
pub const PASSWORD: &str = "relay-password";

/// The line that ends a message's data.
const END_OF_DATA: &str = ".\r\n";

/// The SMTP verbs the relay answers by name; any other gets `250 ok`.
const EHLO: &str = "EHLO";
const AUTH: &str = "AUTH";
const RCPT: &str = "RCPT";
const DATA: &str = "DATA";
const QUIT: &str = "QUIT";

/// What the relay does with one connection.
#[derive(Debug, Clone, Copy)]
pub enum Session {
    /// Refuses the credentials with this reply code.
    RefuseAuth(u16),
    /// Refuses the recipient with this reply code.
    RefuseRecipient(u16),
    /// Takes the whole message, then closes without replying to it.
    DropAfterData,
    /// Never says hello.
    Stall,
    /// Takes the message and accepts it.
    Accept,
}

/// A running relay: where it listens, how many connections it took, and every
/// message it was given.
#[derive(Debug)]
pub struct FakeRelay {
    port: u16,
    connections: Arc<AtomicUsize>,
    received: Arc<Mutex<Vec<String>>>,
}

impl FakeRelay {
    /// Starts a relay playing `script`, one session per connection.
    ///
    /// # Panics
    /// When no loopback port binds: a host that cannot listen on loopback
    /// cannot run the suite, and should stop it loudly.
    pub async fn start(script: Vec<Session>) -> Self {
        let listener = TcpListener::bind((LOOPBACK, 0))
            .await
            .expect("a loopback port binds");
        let port = listener
            .local_addr()
            .expect("a bound listener has an address")
            .port();
        let connections = Arc::new(AtomicUsize::new(0));
        let received = Arc::new(Mutex::new(Vec::new()));
        let (counter, sink) = (Arc::clone(&connections), Arc::clone(&received));
        let mut script = VecDeque::from(script);
        tokio::spawn(async move {
            while let Ok((stream, _peer)) = listener.accept().await {
                counter.fetch_add(1, Ordering::SeqCst);
                let session = script.pop_front().unwrap_or(Session::Accept);
                tokio::spawn(serve(stream, session, Arc::clone(&sink)));
            }
        });
        Self {
            port,
            connections,
            received,
        }
    }

    /// The port the relay listens on.
    #[must_use]
    pub const fn port(&self) -> u16 {
        self.port
    }

    /// How many connections the relay has accepted.
    #[must_use]
    pub fn connections(&self) -> usize {
        self.connections.load(Ordering::SeqCst)
    }

    /// Every message taken so far, raw, in arrival order.
    #[must_use]
    pub fn received(&self) -> Vec<String> {
        self.received
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The relay a sealed `smtp-relay` bag naming this listener parses to.
    #[cfg(test)]
    pub(crate) fn relay(&self) -> crate::relay::Relay {
        let bag =
            afd_crypto::secret::SecretBytes::new(bag_json(LOOPBACK, self.port()).into_bytes());
        crate::relay::Relay::parse(&bag).expect("a complete bag is a relay")
    }
}

/// The `smtp-relay` bag naming a relay at `host:port`, as
/// `playbooks/lib/platform_secret_sync.sh` writes it: five strings.
#[must_use]
pub fn bag_json(host: &str, port: u16) -> String {
    serde_json::json!({
        "host": host,
        "port": port.to_string(),
        "username": USERNAME,
        "password": PASSWORD,
        "from_address": FROM,
    })
    .to_string()
}

async fn serve(stream: TcpStream, session: Session, sink: Arc<Mutex<Vec<String>>>) {
    if matches!(session, Session::Stall) {
        // Holds the socket open and says nothing until the client gives up.
        let _held = stream;
        return std::future::pending().await;
    }
    let (read, mut write) = stream.into_split();
    let mut lines = BufReader::new(read);
    let _greeted = write.write_all(b"220 fake ESMTP\r\n").await;
    let mut line = String::new();
    loop {
        line.clear();
        if lines.read_line(&mut line).await.unwrap_or(0) == 0 {
            return;
        }
        let verb = line.get(..4).unwrap_or_default().to_ascii_uppercase();
        let reply = match (verb.as_str(), session) {
            (EHLO, _) => "250-fake\r\n250-AUTH PLAIN LOGIN\r\n250 8BITMIME\r\n".to_owned(),
            (AUTH, Session::RefuseAuth(code)) => format!("{code} 5.7.8 refused\r\n"),
            (AUTH, _) => "235 2.7.0 ok\r\n".to_owned(),
            (RCPT, Session::RefuseRecipient(code)) => format!("{code} refused\r\n"),
            (DATA, _) => {
                let _go = write.write_all(b"354 go\r\n").await;
                let message = read_message(&mut lines).await;
                sink.lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(message);
                if matches!(session, Session::DropAfterData) {
                    return;
                }
                "250 2.0.0 queued\r\n".to_owned()
            }
            (QUIT, _) => {
                let _bye = write.write_all(b"221 bye\r\n").await;
                return;
            }
            _ => "250 ok\r\n".to_owned(),
        };
        if write.write_all(reply.as_bytes()).await.is_err() {
            return;
        }
    }
}

/// The message body, up to the lone `.` that ends it.
async fn read_message(lines: &mut BufReader<OwnedReadHalf>) -> String {
    let mut message = String::new();
    let mut line = String::new();
    loop {
        line.clear();
        if lines.read_line(&mut line).await.unwrap_or(0) == 0 || line == END_OF_DATA {
            return message;
        }
        message.push_str(&line);
    }
}
