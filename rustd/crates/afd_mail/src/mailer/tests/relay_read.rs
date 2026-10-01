//! The two steps before a message exists: reading the relay from the vault and
//! building a transport to it. Each failure is an outcome closed by one record
//! that says why, and no record says who the invite was for or what it said.

use std::future::{Ready, ready};
use std::sync::Arc;
use std::time::Duration;

use afd_core::error_code;
use afd_core::test_util::trace::{Capture, CapturedEvent};
use afd_crypto::entropy::Entropy;
use afd_crypto::secret::Kek;
use afd_db::test_util::unreachable_db;
use afd_vault::Vault;
use lettre::transport::smtp::client::{TlsParameters, TlsVersion};
use tracing::Level;

use super::{DEADLINE, EVENT_FAILED, INVITE, INVITER, Outcome, Relay, id, invite, send_within};
use crate::deliver::tests::Scripted;
use crate::deliver::tests::loopback::{FakeRelay, Session};
use crate::mailer::{REASON_RELAY_UNREAD, REASON_TLS_SETUP, REASON_UNCONFIGURED};
use crate::{IDEMPOTENCY_HEADER, InviteMailer, SMTP_RELAY_BAG};

/// What the vault read resolves to, already.
type Read = Ready<afd_vault::Result<Option<Relay>>>;

fn read(answer: afd_vault::Result<Option<Relay>>) -> Read {
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
async fn captured(send: impl Future<Output = Outcome>, expected: Outcome) -> Vec<CapturedEvent> {
    let capture = Capture::install();
    assert_eq!(send.await, expected);
    let events = capture.events();
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
    let no_bag = send_within(read(Ok(None)), Relay::transport, &invite, DEADLINE);
    let mailer = unreadable_mailer();
    for events in [
        captured(no_bag, Outcome::Unconfigured).await,
        captured(Box::pin(mailer.send(None, &invite)), Outcome::Unconfigured).await,
    ] {
        let error = the_one(&events, |event| event.level == Level::ERROR);
        assert_eq!(field(error, "event"), Some(EVENT_FAILED));
        assert_eq!(field(error, "reason"), Some(REASON_UNCONFIGURED));
        assert_eq!(field(error, "bag"), Some(SMTP_RELAY_BAG));
        assert_eq!(
            field(error, "error_code"),
            Some(error_code::INVITE_EMAIL_UNAVAILABLE.as_str())
        );
        assert_eq!(field(error, "invite_id"), Some(INVITE));
    }
}

/// A vault that will not answer and a TLS setup that fails each end the send
/// as `failed` with no reply code and no connection made. The closing record
/// names the reason and carries the cause's sentence as `detail`.
#[tokio::test]
async fn test_vault_failure_is_failed_without_detail_leak() {
    let id = id();
    let mailer = unreadable_mailer();
    let unread = captured(
        Box::pin(mailer.send(Some(&id), &invite(&id))),
        Outcome::Failed { reply: None },
    )
    .await;
    let relay = FakeRelay::start(Vec::new()).await;
    let untrusted = captured(
        send_within(
            read(Ok(Some(relay.relay()))),
            |_: &Relay, _: Duration| Err::<Scripted, _>(tls_refusal()),
            &invite(&id),
            DEADLINE,
        ),
        Outcome::Failed { reply: None },
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
    let outcome = send_within(
        read(Ok(Some(relay.relay()))),
        Relay::transport,
        &invite(&id),
        DEADLINE,
    )
    .await;
    assert_eq!(outcome, Outcome::Sent { reply: 250 });
    let [message] = relay
        .received()
        .try_into()
        .expect("the relay took one message");
    assert!(
        message.contains(&format!("{IDEMPOTENCY_HEADER}: invite-{INVITE}-1")),
        "{message}"
    );
}
