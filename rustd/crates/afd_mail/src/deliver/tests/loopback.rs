//! A scripted SMTP relay on loopback, so the unit suite drives lettre's real
//! transport through refusals and a dropped connection.
//!
//! It follows `rustd/crates/afd_api/tests/support/fake_smtp.rs`, which the
//! integration lane runs beside Mailpit; that one lives in another crate's test
//! tree, so this crate keeps its own. Each connection plays the next
//! [`Session`] of the script, connections past the script are accepted, and
//! the relay counts connections so a suite can tell one try from a retry.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use afd_crypto::secret::SecretBytes;
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};
use tokio::net::tcp::OwnedReadHalf;
use tokio::net::{TcpListener, TcpStream};

use crate::relay::Relay;

/// Where the relay listens, and the host its bag names.
const LOOPBACK: &str = "127.0.0.1";

/// The sender the relay's bag names.
const FROM: &str = "hello@agentsfleet.test";

/// What the relay does with one connection.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Session {
    /// Refuses the credentials with this reply code.
    RefuseAuth(u16),
    /// Refuses the recipient with this reply code.
    RefuseRecipient(u16),
    /// Takes the whole message, then closes without replying to it.
    DropAfterData,
    /// Takes the message and accepts it.
    Accept,
}

/// A running relay: where it listens, how many connections it took, and every
/// message it was given.
pub(crate) struct FakeRelay {
    port: u16,
    connections: Arc<AtomicUsize>,
    received: Arc<Mutex<Vec<String>>>,
}

impl FakeRelay {
    /// Starts a relay playing `script`, one session per connection.
    pub(crate) async fn start(script: Vec<Session>) -> Self {
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

    /// The relay a sealed `smtp-relay` bag naming this listener parses to.
    pub(crate) fn relay(&self) -> Relay {
        let bag = serde_json::json!({
            "host": LOOPBACK,
            "port": self.port.to_string(),
            "username": "relay",
            "password": "relay-password",
            "from_address": FROM,
        })
        .to_string();
        Relay::parse(&SecretBytes::new(bag.into_bytes())).expect("a complete bag is a relay")
    }

    /// How many connections the relay has accepted.
    pub(crate) fn connections(&self) -> usize {
        self.connections.load(Ordering::SeqCst)
    }

    /// Every message taken so far, raw, in arrival order.
    pub(crate) fn received(&self) -> Vec<String> {
        self.received
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

async fn serve(stream: TcpStream, session: Session, sink: Arc<Mutex<Vec<String>>>) {
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
            ("EHLO", _) => "250-fake\r\n250-AUTH PLAIN LOGIN\r\n250 8BITMIME\r\n".to_owned(),
            ("AUTH", Session::RefuseAuth(code)) => format!("{code} 5.7.8 refused\r\n"),
            ("AUTH", _) => "235 2.7.0 ok\r\n".to_owned(),
            ("RCPT", Session::RefuseRecipient(code)) => format!("{code} refused\r\n"),
            ("DATA", _) => {
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
            ("QUIT", _) => {
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
        if lines.read_line(&mut line).await.unwrap_or(0) == 0 || line == ".\r\n" {
            return message;
        }
        message.push_str(&line);
    }
}
