#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use super::{TriggerAddRequest, TriggerKind};

#[test]
fn a_kind_serializes_as_its_stored_spelling() {
    for kind in TriggerKind::ALL {
        assert_eq!(
            serde_json::to_value(kind).expect("a kind serializes"),
            serde_json::Value::from(kind.as_str())
        );
    }
}

#[test]
fn a_webhook_add_reads_its_source_verifier_and_secret() {
    let request: TriggerAddRequest<'_> = serde_json::from_str(
        r#"{"fencing_token": 4, "item_id": "019a0000-0000-7000-8000-000000000001",
            "call_number": 2, "kind": "webhook", "source": "sentry",
            "verifier": {"scheme": "body-hex-sha256", "header": "sentry-hook-signature"},
            "secret_name": "sentry_hook"}"#,
    )
    .expect("a webhook add");

    assert_eq!(request.kind, TriggerKind::Webhook);
    assert_eq!(request.source.as_deref(), Some("sentry"));
    assert_eq!(
        request
            .verifier
            .as_ref()
            .map(|verifier| verifier.scheme.as_ref()),
        Some("body-hex-sha256")
    );
    assert_eq!(request.secret_name.as_deref(), Some("sentry_hook"));
    assert!(request.channel.is_none());
}

#[test]
fn an_unknown_kind_or_key_is_refused() {
    for body in [
        r#"{"fencing_token": 4, "item_id": "i", "call_number": 1, "kind": "cron"}"#,
        r#"{"fencing_token": 4, "item_id": "i", "call_number": 1, "kind": "Webhook"}"#,
        r#"{"fencing_token": 4, "item_id": "i", "call_number": 1, "kind": "mention", "channels": ["C0123456789"]}"#,
        r#"{"fencing_token": 4, "item_id": "i", "call_number": 1, "kind": "webhook", "verifier": {"scheme": "svix-v1", "secret": "x"}}"#,
        r#"{"item_id": "i", "call_number": 1, "kind": "webhook"}"#,
    ] {
        assert!(
            serde_json::from_str::<TriggerAddRequest<'_>>(body).is_err(),
            "{body}"
        );
    }
}
