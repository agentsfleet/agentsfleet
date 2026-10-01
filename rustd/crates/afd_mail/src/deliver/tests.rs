#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use std::sync::Mutex;
use std::time::Duration;

use lettre::message::header::Subject;
use lettre::transport::stub::AsyncStubTransport;
use lettre::{AsyncTransport as _, Message};

use super::{
    Attempted, Delivery, IDEMPOTENCY_HEADER, IdempotencyKey, Mailer, message, send_once_retrying,
};
use crate::{InviteLetter, RenderedEmail, render_invite};

pub(crate) mod loopback;

use self::loopback::{FakeRelay, Session};

const ACCEPTED: u16 = 250;

/// How long each SMTP command may take against the loopback relay.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

/// What a fixture's `expect` says when its fixed input fails to parse.
const MAILBOX: &str = "a mailbox";

/// The invitee every message here is addressed to.
const RECIPIENT: &str = "bob@example.test";

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
    built_from(&rendered(), key)
}

fn built_from(rendered: &RenderedEmail, key: IdempotencyKey) -> Message {
    message(
        "agentsfleet <hello@agentsfleet.test>"
            .parse()
            .expect(MAILBOX),
        RECIPIENT.parse().expect(MAILBOX),
        rendered,
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
        [RECIPIENT]
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

/// Sends one message through lettre's real transport to `relay`, the way a
/// production send reaches its relay.
async fn send_over_loopback(relay: &FakeRelay) -> Attempted {
    let transport = relay
        .relay()
        .transport(COMMAND_TIMEOUT)
        .expect("a loopback transport builds");
    send_once_retrying(&transport, built(IdempotencyKey::for_invite("i", 1))).await
}

/// Each reply a relay can speak maps to the delivery the invite records, over
/// lettre's real transport. A refused credential, a transient refusal and a
/// permanent one are each tried once and keep their code; only a connection
/// that closed before the relay answered is tried again, with the same message,
/// and only once.
#[tokio::test]
async fn test_classify_reply_codes_over_loopback() {
    for (session, code) in [
        (Session::RefuseAuth(535), 535),
        (Session::RefuseRecipient(450), 450),
        (Session::RefuseRecipient(550), 550),
    ] {
        let relay = FakeRelay::start(vec![session]).await;
        let refused = Attempted {
            delivery: Delivery::Refused { reply: Some(code) },
            retried: false,
        };
        assert_eq!(send_over_loopback(&relay).await, refused, "{session:?}");
        assert_eq!(relay.connections(), 1, "{session:?}");
    }

    let recovering = FakeRelay::start(vec![Session::DropAfterData, Session::Accept]).await;
    let recovered = Attempted {
        delivery: Delivery::Accepted { reply: ACCEPTED },
        retried: true,
    };
    assert_eq!(send_over_loopback(&recovering).await, recovered);
    assert_eq!(recovering.connections(), 2);
    let [first, second] = recovering
        .received()
        .try_into()
        .expect("the relay took both copies");
    assert_eq!(first, second);

    let dropping = FakeRelay::start(vec![Session::DropAfterData, Session::DropAfterData]).await;
    let unreachable = Attempted {
        delivery: Delivery::Unreachable,
        retried: true,
    };
    assert_eq!(send_over_loopback(&dropping).await, unreachable);
    assert_eq!(dropping.connections(), 2);
}

/// A display name carrying a line break and a header cannot add a header: the
/// subject is encoded whole, so the name stays inside it, the header block has
/// no `Bcc:` line, and the invitee is the only recipient.
#[test]
fn test_crlf_in_owner_name_stays_in_subject() {
    let hostile = "John\r\nBcc: evil@example.com";
    let rendered = render_invite(&InviteLetter {
        inviter_name: hostile,
        owner_name: hostile,
        invite_url: "https://app.agentsfleet.test/invites/x",
    })
    .expect("the invite renders");
    let sent = built_from(&rendered, IdempotencyKey::for_invite("i", 1));

    let formatted = String::from_utf8(sent.formatted()).expect("an encoded message is ASCII");
    let (headers, _body) = formatted
        .split_once("\r\n\r\n")
        .expect("a header block ends at the first blank line");
    assert!(
        headers
            .lines()
            .all(|line| !line.to_ascii_lowercase().starts_with("bcc:")),
        "{headers}"
    );
    assert_eq!(
        sent.headers().get::<Subject>(),
        Some(Subject::from(rendered.subject.clone()))
    );
    assert_eq!(
        sent.envelope()
            .to()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        [RECIPIENT]
    );
}

/// The header's parse half reads back the key its display half wrote, so a
/// typed read of a built message names the same invite and attempt.
#[test]
fn should_read_back_idempotency_key_when_message_built() {
    let key = IdempotencyKey::for_invite("0190f5a2-4b2d-7c11-8d5e-2a5f31d98210", 3);
    assert_eq!(
        built(key.clone()).headers().get::<IdempotencyKey>(),
        Some(key)
    );
}
