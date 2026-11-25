//! A trigger row as a person reads it: a webhook carries the address its
//! deliveries reach, and no other kind carries one.
#![expect(clippy::expect_used, reason = "test preconditions must fail loudly")]

use af_core::id::Uuid7;
use af_wire::trigger_verb::TriggerKind;

use super::TriggerRow;

/// The deployment every address is rendered against.
const DEPLOYMENT: &str = "https://api.example.test";
/// The fleet every address names.
const FLEET: &str = "01920000-0000-7000-8000-000000000001";

fn row(trigger_type: TriggerKind, source: &str) -> TriggerRow {
    TriggerRow {
        id: Uuid7::parse("01920000-0000-7000-8000-0000000000a1").expect("a UUIDv7"),
        trigger_type,
        source: source.to_owned(),
        events: Some(vec!["issue".to_owned()]),
        repositories: None,
        verifier_scheme: None,
        verifier_header: None,
        secret_name: Some("sentry_hook".to_owned()),
        channel: None,
        created_at: jiff::Timestamp::UNIX_EPOCH,
    }
}

#[test]
fn a_webhook_row_carries_the_address_its_route_serves() {
    let fleet = Uuid7::parse(FLEET).expect("a UUIDv7");
    let stored = row(TriggerKind::Webhook, "sentry");

    let shown = stored.shown(DEPLOYMENT, &fleet);

    assert_eq!(
        shown.url.as_deref(),
        Some("https://api.example.test/v1/webhooks/01920000-0000-7000-8000-000000000001/sentry")
    );
    assert_eq!(shown.source, "sentry");
    assert_eq!(shown.events.as_deref().map(<[_]>::len), Some(1));
}

/// An App subscription and a mention are woken through routes no fleet is
/// addressed on, so neither is handed an address.
#[test]
fn an_app_event_or_mention_row_carries_no_address() {
    let fleet = Uuid7::parse(FLEET).expect("a UUIDv7");
    for (kind, source) in [
        (TriggerKind::AppEvent, "github"),
        (TriggerKind::Mention, "slack"),
    ] {
        let stored = row(kind, source);
        assert_eq!(stored.shown(DEPLOYMENT, &fleet).url, None, "{source}");
    }
}

/// The wire never carries a webhook's signing secret's name.
#[test]
fn a_shown_trigger_names_no_signing_secret() {
    let fleet = Uuid7::parse(FLEET).expect("a UUIDv7");
    let stored = row(TriggerKind::Webhook, "sentry");

    let body =
        serde_json::to_string(&stored.shown(DEPLOYMENT, &fleet)).expect("a trigger serializes");

    assert!(!body.contains("sentry_hook"), "{body}");
}
