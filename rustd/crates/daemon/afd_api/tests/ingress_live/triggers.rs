//! A webhook trigger row the fleet added routes a signed delivery to it, and
//! nothing else does.

use super::fixture::{Fixture, Runnable, SIGNING_SECRET, Secret, WRONG_SECRET};
use super::{harness, json_body, signed};
use af_wire::trigger_verb::TriggerKind;
use http::{Method, StatusCode};
use serde_json::{Value, json};

/// The sender a fleet added a webhook for.
const SENTRY: &str = "sentry";

/// The vault key its signing secret is stored under.
const SENTRY_HOOK: &str = "sentry-hook";

/// The header Sentry's recipe reads its signature from.
const SENTRY_HEADER: &str = "sentry-hook-signature";

/// What Sentry sends.
const ISSUE: &str = r#"{"action":"created","data":{"issue":{"id":"7"}}}"#;

/// The row `trigger_add {kind:"webhook", source:"sentry", verifier,
/// secret_name:"sentry-hook"}` writes, and the signing secret it names.
async fn sentry(fixture: &Fixture) -> af_core::id::Uuid7 {
    let text = str::from_utf8(SIGNING_SECRET).expect("the fixture secret is text");
    fixture
        .seal(SENTRY_HOOK, &json!({"webhook_secret": text}).to_string())
        .await;
    harness::triggers::seed(
        fixture.database(),
        &fixture.fleet,
        &fixture.workspace,
        harness::triggers::Seeded {
            verifier: Some(("body-hex-sha256", Some(SENTRY_HEADER))),
            secret_name: Some(SENTRY_HOOK),
            ..harness::triggers::Seeded::new(TriggerKind::Webhook, SENTRY)
        },
    )
    .await
}

/// One delivery of [`ISSUE`] to `source`, signed with `secret`.
async fn deliver(
    router: &axum::Router,
    fixture: &Fixture,
    source: &str,
    secret: &[u8],
) -> axum::response::Response {
    let proof = signed::signature(af_webhook::Scheme::BodyHex, secret, ISSUE.as_bytes());
    let headers = vec![(signed::name(SENTRY_HEADER), proof.as_str())];
    let path = format!("/v1/webhooks/{}/{source}", fixture.fleet);
    harness::send_with_headers(router, Method::POST, &path, None, ISSUE, &headers).await
}

/// A signed delivery to the URL of a webhook row the fleet added makes one
/// turn, admitted from the webhook.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_webhook_trigger_admits_a_signed_delivery() {
    let fixture = Fixture::create().await;
    fixture.seed(Runnable::Active, Secret::Stored).await;
    sentry(&fixture).await;
    let router = fixture.router();

    let woken = deliver(&router, &fixture, SENTRY, SIGNING_SECRET).await;
    let status = woken.status();
    let document = json_body(woken).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{document}");
    assert_eq!(
        document.get("replayed").and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        fixture.turn_actors().await,
        vec![format!("webhook:{SENTRY}")],
        "one turn, admitted from the webhook"
    );
    fixture.cleanup().await;
}

/// A wrong signature on a trigger's URL is refused with 401 and makes no
/// turn; a source no row names, and a removed row's, answer 404.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn an_unsigned_delivery_to_a_trigger_is_refused() {
    let fixture = Fixture::create().await;
    fixture.seed(Runnable::Active, Secret::Stored).await;
    let row = sentry(&fixture).await;
    let router = fixture.router();

    let forged = deliver(&router, &fixture, SENTRY, WRONG_SECRET).await;
    assert_eq!(forged.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        json_body(forged).await.get("error_code"),
        Some(&json!("UZ-WH-010"))
    );
    let unnamed = deliver(&router, &fixture, "pagerduty", SIGNING_SECRET).await;
    assert_eq!(unnamed.status(), StatusCode::NOT_FOUND);

    sqlx::query("DELETE FROM fleet.triggers WHERE id = $1")
        .bind(&row)
        .execute(&mut *fixture.database().acquire().await.expect("a connection"))
        .await
        .expect("the row is removed");
    let removed = deliver(&router, &fixture, SENTRY, SIGNING_SECRET).await;
    assert_eq!(removed.status(), StatusCode::NOT_FOUND);
    assert!(
        fixture.turn_actors().await.is_empty(),
        "no refused delivery makes a turn"
    );
    fixture.cleanup().await;
}
