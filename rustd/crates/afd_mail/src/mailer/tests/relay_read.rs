//! The two steps before a message exists: reading the relay from the vault and
//! building a transport to it. Each failure is an outcome closed by one record
//! that says why, and no record says who the invite was for or what it said.

use std::future::{Ready, ready};
use std::sync::Arc;
use std::time::Duration;

use afd_core::error_code;
use afd_core::test_util::trace::{Capture, CapturedEvent};
use afd_crypto::entropy::Entropy;
use afd_crypto::secret::{Kek, SecretBytes};
use afd_db::test_util::unreachable_db;
use afd_observability::InviteEmailOutcome;
use afd_vault::Vault;
use lettre::transport::smtp::client::{TlsParameters, TlsVersion};
use tracing::Level;

use super::{
    DEADLINE, EVENT_FAILED, INVITE, STALL_DEADLINE, assert_no_address, connect, id, invite,
    send_within,
};
use crate::deliver::tests::Scripted;
use crate::mailer::{
    REASON_DEADLINE, REASON_RELAY_UNREAD, REASON_TLS_SETUP, REASON_UNCONFIGURED, RelayRead,
};
use crate::relay::{BagFault, Relay, Server};
use crate::test_util::{FakeRelay, LOOPBACK, PASSWORD, Session, bag_json};
use crate::{IDEMPOTENCY_HEADER, InviteMailer, SMTP_RELAY_BAG};

/// What the vault read resolves to, already.
type Read = Ready<RelayRead>;

fn read(answer: RelayRead) -> Read {
    ready(answer)
}

/// A mailer whose vault sits on a Postgres nobody answers on, so reading the
/// relay fails the way a datastore outage would in production.
fn unreadable_mailer() -> InviteMailer {
    let kek = Arc::new(Kek::from_bytes([7; 32]));
    InviteMailer::new(Vault::new(unreachable_db(), kek, Entropy::new())).with_deadline(DEADLINE)
}

/// A TLS setup rustls refuses, because TLS 1.0 is below what it will speak.
fn tls_refusal() -> lettre::transport::smtp::Error {
    TlsParameters::builder(INVITE.to_owned())
        .set_min_tls_version(TlsVersion::Tlsv10)
        .build_rustls()
        .err()
        .expect("rustls refuses TLS 1.0")
}

/// Runs `send` under a capture, checks its outcome, and returns every record
/// it raised once none of them names the invitee or quotes the email.
async fn captured(
    send: impl Future<Output = InviteEmailOutcome>,
    expected: InviteEmailOutcome,
) -> Vec<CapturedEvent> {
    let capture = Capture::install();
    assert_eq!(send.await, expected);
    let events = capture.events();
    assert_no_address(&events);
    events
}

/// The one record `matches` picks out of `events`.
fn the_one(events: &[CapturedEvent], matches: impl Fn(&CapturedEvent) -> bool) -> &CapturedEvent {
    let [one]: [&CapturedEvent; 1] = events
        .iter()
        .filter(|event| matches(event))
        .collect::<Vec<_>>()
        .try_into()
        .expect("exactly one record matches");
    one
}

fn field<'e>(event: &'e CapturedEvent, name: &str) -> Option<&'e str> {
    event.fields.get(name).map(String::as_str)
}

/// No usable relay is one error-level record naming the bag an operator has to
/// seal, whether the vault held no bag or there was no admin workspace to ask.
#[tokio::test]
async fn test_unconfigured_logs_one_error_naming_bag() {
    let id = id();
    let invite = invite(&id);
    let absent = send_within(read(Ok(Err(BagFault::Absent))), connect, &invite, DEADLINE);
    let mailer = unreadable_mailer();
    let unconfigured = InviteEmailOutcome::Unconfigured;
    for events in [
        captured(absent, unconfigured).await,
        captured(Box::pin(mailer.send(None, &invite)), unconfigured).await,
    ] {
        let error = the_one(&events, |event| event.level == Level::ERROR);
        assert_eq!(field(error, "event"), Some(EVENT_FAILED));
        assert_eq!(field(error, "reason"), Some(REASON_UNCONFIGURED));
        assert_eq!(field(error, "bag"), Some(SMTP_RELAY_BAG));
        // pin test: literal is the contract
        assert_eq!(field(error, "fault"), Some("absent"));
        assert_eq!(field(error, "field"), None);
        assert_eq!(
            field(error, "error_code"),
            Some(error_code::INVITE_EMAIL_UNAVAILABLE.as_str())
        );
        assert_eq!(field(error, "invite_id"), Some(INVITE));
    }
}

/// A bag that is there but unusable is still `unconfigured`, and its record
/// names the fault and the field to fix — never a value from the bag, the
/// password least of all.
#[tokio::test]
async fn should_name_fault_not_value_when_bag_unusable() {
    let id = id();
    let invite = invite(&id);
    let json = bag_json(LOOPBACK, 1).replace("\"1\"", "\"not a port\"");
    let unusable = Relay::parse(&SecretBytes::new(json.into_bytes()));
    let events = captured(
        send_within(read(Ok(unusable)), connect, &invite, DEADLINE),
        InviteEmailOutcome::Unconfigured,
    )
    .await;
    let error = the_one(&events, |event| event.level == Level::ERROR);
    // pin test: literal is the contract
    assert_eq!(field(error, "fault"), Some("unparsed"));
    assert_eq!(field(error, "field"), Some("port"));
    for value in events.iter().flat_map(|event| event.fields.values()) {
        assert!(!value.contains(PASSWORD), "{value}");
        assert!(!value.contains("not a port"), "{value}");
    }
}

/// A vault that will not answer and a TLS setup that fails each end the send
/// as `failed` with no reply code and no connection made. The closing record
/// names the reason and carries the cause's sentence as `detail`.
#[tokio::test]
async fn test_vault_failure_is_failed_without_detail_leak() {
    let id = id();
    let mailer = unreadable_mailer();
    let failed = InviteEmailOutcome::Failed { reply: None };
    let unread = captured(Box::pin(mailer.send(Some(&id), &invite(&id))), failed).await;
    let relay = FakeRelay::start(Vec::new()).await;
    let untrusted = captured(
        send_within(
            read(Ok(Ok(relay.relay()))),
            |_: Server, _: Duration| Err::<Scripted, _>(tls_refusal()),
            &invite(&id),
            DEADLINE,
        ),
        failed,
    )
    .await;
    for (events, reason) in [(unread, REASON_RELAY_UNREAD), (untrusted, REASON_TLS_SETUP)] {
        let failed = the_one(&events, |event| field(event, "event") == Some(EVENT_FAILED));
        assert_eq!(failed.level, Level::WARN, "{failed:?}");
        assert_eq!(field(failed, "reason"), Some(reason), "{failed:?}");
        assert_eq!(field(failed, "reply"), None, "{failed:?}");
        assert!(
            field(failed, "detail").is_some_and(|detail| !detail.is_empty()),
            "{failed:?}"
        );
    }
    assert_eq!(relay.connections(), 0);
}

/// A read naming a relay that answers sends through lettre's real transport,
/// and the relay takes one message keyed to this invite and attempt.
#[tokio::test]
async fn should_send_when_read_names_a_live_relay() {
    let relay = FakeRelay::start(vec![Session::Accept]).await;
    let id = id();
    let outcome = send_within(read(Ok(Ok(relay.relay()))), connect, &invite(&id), DEADLINE).await;
    assert_eq!(outcome, InviteEmailOutcome::Sent { reply: 250 });
    let [message] = relay
        .received()
        .try_into()
        .expect("the relay took one message");
    assert!(
        message.contains(&format!("{IDEMPOTENCY_HEADER}: invite-{INVITE}-1")),
        "{message}"
    );
}

/// A relay that takes the connection and never says hello is cut off at the
/// deadline over lettre's real transport: `failed`, no reply code, the record
/// naming the deadline, and nothing delivered.
#[tokio::test]
async fn should_fail_at_deadline_when_relay_never_greets() {
    let relay = FakeRelay::start(vec![Session::Stall]).await;
    let id = id();
    let events = captured(
        send_within(
            read(Ok(Ok(relay.relay()))),
            connect,
            &invite(&id),
            STALL_DEADLINE,
        ),
        InviteEmailOutcome::Failed { reply: None },
    )
    .await;
    let failed = the_one(&events, |event| field(event, "event") == Some(EVENT_FAILED));
    assert_eq!(field(failed, "reason"), Some(REASON_DEADLINE), "{failed:?}");
    assert_eq!(relay.connections(), 1);
    assert!(relay.received().is_empty());
}
