#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use std::time::Duration;

use afd_core::id::Uuid7;
use afd_core::test_util::trace::{Capture, CapturedEvent};
use afd_observability::InviteEmailOutcome;
use lettre::address::Envelope;
use lettre::message::Mailbox;

use super::{
    Attempt, EVENT_COMPLETED, EVENT_FAILED, EVENT_RETRIED, EVENT_STARTED, InviteSend, send_with,
    send_within,
};
use crate::InviteLetter;
use crate::deliver::tests::{DOMAIN_LITERAL, INVITE, INVITE_URL, RECIPIENT, Scripted};
use crate::deliver::{Delivery, Mailer};
use crate::relay::{Server, TlsCache};
use crate::test_util::{FROM, VALID_DAYS};

mod relay_read;

const INVITER: &str = "John";
const DEADLINE: Duration = Duration::from_secs(5);
const STALL_DEADLINE: Duration = Duration::from_millis(50);

/// A relay that never answers.
struct Stalled;

impl Mailer for Stalled {
    async fn deliver(&self, _envelope: &Envelope, _raw: &[u8]) -> Delivery {
        std::future::pending().await
    }
}

fn from() -> Mailbox {
    FROM.parse().expect("a mailbox")
}

fn invite(id: &Uuid7) -> InviteSend<'_> {
    InviteSend {
        invite_id: id,
        attempt: 1,
        to: RECIPIENT,
        letter: InviteLetter {
            inviter_name: INVITER,
            owner_name: INVITER,
            invite_url: INVITE_URL,
            valid_days: VALID_DAYS,
        },
    }
}

fn id() -> Uuid7 {
    Uuid7::parse(INVITE).expect("a UUIDv7")
}

/// The production connect, over a fresh TLS cache.
fn connect(
    server: Server,
    timeout: Duration,
) -> crate::Result<lettre::AsyncSmtpTransport<lettre::Tokio1Executor>, lettre::transport::smtp::Error>
{
    server.transport(timeout, &TlsCache::default())
}

/// Runs one send through `mailer` the way `InviteMailer::send` does.
async fn run<M: Mailer>(
    mailer: &M,
    invite: &InviteSend<'_>,
    deadline: Duration,
) -> InviteEmailOutcome {
    send_with(mailer, from(), &Attempt::begin(invite), deadline).await
}

/// No record names the invitee or quotes the email: no address, no body text.
fn assert_no_address(events: &[CapturedEvent]) {
    for value in events.iter().flat_map(|event| event.fields.values()) {
        assert!(
            !value.contains('@'),
            "an address reached a log record: {value}"
        );
        assert!(
            !value.contains(INVITER),
            "body text reached a log record: {value}"
        );
    }
}

/// Dimension 2.3: no record holds the recipient's address or any body text —
/// only the invite id, the attempt, the reason and the reply.
#[tokio::test]
async fn test_send_logs_carry_no_address() {
    let capture = Capture::install();
    let id = id();
    let accepting = Scripted::new(vec![Delivery::Accepted { reply: 250 }]);
    assert_eq!(
        run(&accepting, &invite(&id), DEADLINE).await,
        InviteEmailOutcome::Sent { reply: 250 }
    );
    let refusing = Scripted::new(vec![Delivery::Refused { reply: Some(550) }]);
    assert_eq!(
        run(&refusing, &invite(&id), DEADLINE).await,
        InviteEmailOutcome::Failed { reply: Some(550) }
    );

    let completed = capture.only(EVENT_COMPLETED);
    let failed = capture.only(EVENT_FAILED);
    assert_eq!(
        completed.fields.get("invite_id").map(String::as_str),
        Some(INVITE)
    );
    assert_eq!(failed.fields.get("reply").map(String::as_str), Some("550"));
    assert_no_address(&capture.events());
}

/// Every send opens with `invite_email_started` and closes with exactly one
/// of completed or failed, each carrying `duration_ms` (`docs/LOGGING_STANDARD.md` §4).
#[tokio::test]
async fn every_send_is_bracketed() {
    let id = id();
    for answers in [
        vec![Delivery::Accepted { reply: 250 }],
        vec![Delivery::Refused { reply: Some(535) }],
        vec![Delivery::Unreachable, Delivery::Unreachable],
    ] {
        let capture = Capture::install();
        let _outcome = run(&Scripted::new(answers), &invite(&id), DEADLINE).await;
        let names: Vec<String> = capture
            .events()
            .into_iter()
            .filter_map(|event| event.fields.get("event").cloned())
            .filter(|name| name != EVENT_RETRIED)
            .collect();
        assert_eq!(names.first().map(String::as_str), Some(EVENT_STARTED));
        assert_eq!(names.len(), 2, "{names:?}");
        let closing = capture
            .events()
            .into_iter()
            .last()
            .expect("a closing record");
        assert!(closing.fields.contains_key("duration_ms"));
    }
}

/// A first try that never reached the relay logs one retry, recovered or not.
#[tokio::test]
async fn a_retry_is_logged_once() {
    let capture = Capture::install();
    let id = id();
    let flaky = Scripted::new(vec![
        Delivery::Unreachable,
        Delivery::Accepted { reply: 250 },
    ]);
    assert_eq!(
        run(&flaky, &invite(&id), DEADLINE).await,
        InviteEmailOutcome::Sent { reply: 250 }
    );
    let retried = capture.only(EVENT_RETRIED);
    assert_eq!(
        retried.fields.get("recovered").map(String::as_str),
        Some("true")
    );
}

/// Every relay answer maps to the outcome the invite records.
#[tokio::test]
async fn relay_answers_map_to_outcomes() {
    let id = id();
    let cases = [
        (
            vec![Delivery::Accepted { reply: 250 }],
            InviteEmailOutcome::Sent { reply: 250 },
        ),
        (
            vec![Delivery::Refused { reply: Some(535) }],
            InviteEmailOutcome::Failed { reply: Some(535) },
        ),
        (
            vec![Delivery::Refused { reply: Some(450) }],
            InviteEmailOutcome::Failed { reply: Some(450) },
        ),
        (
            vec![Delivery::Unreachable, Delivery::Unreachable],
            InviteEmailOutcome::Failed { reply: None },
        ),
        (
            vec![Delivery::Unreachable, Delivery::Accepted { reply: 250 }],
            InviteEmailOutcome::Sent { reply: 250 },
        ),
    ];
    for (answers, expected) in cases {
        let mailer = Scripted::new(answers);
        assert_eq!(run(&mailer, &invite(&id), DEADLINE).await, expected);
    }
}

/// A relay that never answers is cut off at the deadline as `failed`.
#[tokio::test]
async fn a_stalled_relay_fails_at_the_deadline() {
    let id = id();
    assert_eq!(
        run(&Stalled, &invite(&id), STALL_DEADLINE).await,
        InviteEmailOutcome::Failed { reply: None }
    );
}

/// How long a suite waits for a send that should already have ended at its
/// own deadline before calling it hung.
const HUNG_AFTER: Duration = Duration::from_secs(2);

/// A vault read that never answers is cut off by the same deadline as the
/// relay. The deadline used to start after the read, so a stalled vault held
/// the owner's click open without bound.
#[tokio::test]
async fn should_fail_within_deadline_when_vault_read_stalls() {
    let id = id();
    let stalled = std::future::pending::<super::RelayRead>();
    let outcome = tokio::time::timeout(
        HUNG_AFTER,
        send_within(stalled, connect, &invite(&id), STALL_DEADLINE),
    )
    .await
    .expect("the send ends at its own deadline, not the suite's");
    assert_eq!(outcome, InviteEmailOutcome::Failed { reply: None });
}

/// An address is deliverable exactly when the recipient parse accepts it, so
/// an address shaped like one but refused here never becomes an invite.
#[test]
fn deliverable_is_the_recipient_parse() {
    assert!(super::deliverable(RECIPIENT));
    let past_local_limit = format!("{}@example.test", "a".repeat(65));
    for refused in [
        "not an address",
        "a<b@example.test",
        past_local_limit.as_str(),
    ] {
        assert!(!super::deliverable(refused), "{refused:?} was deliverable");
    }
}

/// An address the builder refuses fails this send without reaching a relay.
#[tokio::test]
async fn an_unparseable_recipient_fails_before_delivery() {
    let id = id();
    let mut bad = invite(&id);
    bad.to = "not an address";
    let mailer = Scripted::new(vec![Delivery::Accepted { reply: 250 }]);
    assert_eq!(
        run(&mailer, &bad, DEADLINE).await,
        InviteEmailOutcome::Failed { reply: None }
    );
    assert!(mailer.formatted().is_empty());
}

/// A domain literal is refused at the route, because no send can address it:
/// it used to pass `deliverable` and then fail every send, since the builder
/// re-reads the recipient with a stricter parser than the guard ran. One that
/// reached a send anyway — an invite stored before the guard agreed — fails
/// before any relay is dialled.
#[tokio::test]
async fn should_refuse_domain_literal_the_builder_cannot_address() {
    assert!(!super::deliverable(DOMAIN_LITERAL));
    let id = id();
    let mut literal = invite(&id);
    literal.to = DOMAIN_LITERAL;
    let mailer = Scripted::new(vec![Delivery::Accepted { reply: 250 }]);
    assert_eq!(
        run(&mailer, &literal, DEADLINE).await,
        InviteEmailOutcome::Failed { reply: None }
    );
    assert!(mailer.formatted().is_empty());
}
