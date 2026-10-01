#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use std::sync::Mutex;

use lettre::transport::stub::AsyncStubTransport;
use lettre::{AsyncTransport as _, Message};

use super::{
    Attempted, Delivery, IDEMPOTENCY_HEADER, IdempotencyKey, Mailer, message, send_once_retrying,
};
use crate::RenderedEmail;

const ACCEPTED: u16 = 250;

/// What a fixture's `expect` says when its fixed input fails to parse.
const MAILBOX: &str = "a mailbox";

/// What a test lock's `expect` says: no test thread panics holding it.
const UNPOISONED: &str = "unpoisoned";

impl Mailer for AsyncStubTransport {
    async fn deliver(&self, message: Message) -> Delivery {
        match self.send(message).await {
            Ok(()) => Delivery::Accepted { reply: ACCEPTED },
            Err(_stub) => Delivery::Refused { reply: None },
        }
    }
}

/// Answers each delivery from a script and records every message.
pub(crate) struct Scripted {
    answers: Mutex<Vec<Delivery>>,
    pub(crate) seen: Mutex<Vec<Message>>,
}

impl Scripted {
    pub(crate) fn new(mut answers: Vec<Delivery>) -> Self {
        answers.reverse();
        Self {
            answers: Mutex::new(answers),
            seen: Mutex::new(Vec::new()),
        }
    }
}

impl Mailer for Scripted {
    fn deliver(&self, message: Message) -> impl Future<Output = Delivery> + Send {
        self.seen.lock().expect(UNPOISONED).push(message);
        let answer = self.answers.lock().expect(UNPOISONED).pop();
        std::future::ready(answer.unwrap_or(Delivery::Unreachable))
    }
}

impl Scripted {
    /// Every message delivered so far, as the bytes a relay would read.
    pub(crate) fn formatted(&self) -> Vec<Vec<u8>> {
        self.seen
            .lock()
            .expect(UNPOISONED)
            .iter()
            .map(Message::formatted)
            .collect()
    }
}

pub(crate) fn rendered() -> RenderedEmail {
    RenderedEmail {
        subject: "You're invited".to_owned(),
        html: "<p>Join</p>".to_owned(),
        text: "Join".to_owned(),
    }
}

fn built(key: IdempotencyKey) -> Message {
    message(
        "agentsfleet <hello@agentsfleet.test>"
            .parse()
            .expect(MAILBOX),
        "bob@example.test".parse().expect(MAILBOX),
        &rendered(),
        key,
    )
    .expect("the message builds")
}

/// Dimension 2.6: the stub transport records one message carrying the
/// envelope, the subject, the idempotency header and both parts.
#[tokio::test]
async fn test_deliver_builds_message() {
    let stub = AsyncStubTransport::new_ok();
    let key = IdempotencyKey::for_invite("0190f5a2-4b2d-7c11-8d5e-2a5f31d98210", 1);
    let answer = send_once_retrying(&stub, built(key)).await;
    assert_eq!(answer.delivery, Delivery::Accepted { reply: ACCEPTED });

    let sent = stub.messages().await;
    let [(envelope, raw)] = sent.as_slice() else {
        panic!("expected one message, got {}", sent.len());
    };
    assert_eq!(
        envelope.from().map(ToString::to_string).as_deref(),
        Some("hello@agentsfleet.test")
    );
    assert_eq!(
        envelope
            .to()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        ["bob@example.test"]
    );
    assert!(raw.contains("Subject: You're invited"));
    assert!(raw.contains(&format!(
        "{IDEMPOTENCY_HEADER}: invite-0190f5a2-4b2d-7c11-8d5e-2a5f31d98210-1"
    )));
    assert!(raw.contains("Content-Type: multipart/alternative"));
    assert!(raw.contains("Content-Type: text/plain"));
    assert!(raw.contains("Content-Type: text/html"));
}

/// A connection that dropped is retried once with the very same message, so
/// the same idempotency key; a refusal is not retried.
#[tokio::test]
async fn only_an_unreachable_relay_is_retried() {
    let key = IdempotencyKey::for_invite("i", 2);
    let flaky = Scripted::new(vec![
        Delivery::Unreachable,
        Delivery::Accepted { reply: ACCEPTED },
    ]);
    let recovered = Attempted {
        delivery: Delivery::Accepted { reply: ACCEPTED },
        retried: true,
    };
    assert_eq!(
        send_once_retrying(&flaky, built(key.clone())).await,
        recovered
    );
    let [first, second] = flaky.formatted().try_into().expect("two deliveries");
    assert_eq!(first, second);

    let refusing = Scripted::new(vec![Delivery::Refused { reply: Some(550) }]);
    let refused = Attempted {
        delivery: Delivery::Refused { reply: Some(550) },
        retried: false,
    };
    assert_eq!(send_once_retrying(&refusing, built(key)).await, refused);
    assert_eq!(refusing.formatted().len(), 1);
}
