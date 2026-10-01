//! A scripted SMTP relay on loopback, for the answers Mailpit will not give.
//!
//! Mailpit accepts everything, which proves delivery and nothing else. This
//! relay plays one [`Session`] per connection, in order — refuse at a stage,
//! drop the connection after the message, stall, or accept — and records each
//! message it was given, so a suite can assert what the retry and the
//! send-again carried. Connections past the script are accepted.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, PoisonError};

use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};
use tokio::net::{TcpListener, TcpStream};

/// What the relay does with one connection.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Session {
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

/// A running relay: where it listens, and every message it took.
pub(crate) struct FakeRelay {
    pub(crate) port: u16,
    received: Arc<Mutex<Vec<String>>>,
}

impl FakeRelay {
    /// Starts a relay playing `script`, one session per connection.
    pub(crate) async fn start(script: Vec<Session>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a loopback port binds");
        let port = listener
            .local_addr()
            .expect("a bound listener has an address")
            .port();
        let received = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&received);
        let mut script = VecDeque::from(script);
        tokio::spawn(async move {
            while let Ok((stream, _peer)) = listener.accept().await {
                let session = script.pop_front().unwrap_or(Session::Accept);
                tokio::spawn(serve(stream, session, Arc::clone(&sink)));
            }
        });
        Self { port, received }
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
    if matches!(session, Session::Stall) {
        // Holds the socket open and says nothing until the client gives up.
        let _held = stream;
        std::future::pending::<()>().await;
        return;
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
async fn read_message(lines: &mut BufReader<tokio::net::tcp::OwnedReadHalf>) -> String {
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
