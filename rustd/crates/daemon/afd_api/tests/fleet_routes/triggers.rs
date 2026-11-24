//! What may wake a fleet, as a person sees and removes it.

use super::fixture::Fixture;
use super::{json_body, send};
use crate::harness::triggers::{Seeded, seed};
use af_core::id::Uuid7;
use af_wire::trigger_verb::TriggerKind;
use http::{Method, StatusCode};
use serde_json::{Value, json};

/// A person lists a fleet's triggers, a webhook's carrying its address and
/// no signing secret, and removes one; a repeat removal answers 204 too.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_person_lists_and_removes_a_fleets_triggers() {
    let f = Fixture::create().await;
    let router = f.router();
    let created = f.create_fleet(&router, &f.token).await;
    let fleet = created["fleet"]["id"]
        .as_str()
        .expect("the fleet's id")
        .to_owned();
    let fleet_id = Uuid7::parse(&fleet).expect("a UUIDv7");
    let hook = seed(
        &f.db,
        &fleet_id,
        &f.workspace,
        Seeded {
            secret_name: Some("sentry-hook"),
            verifier: Some(("body-hex-sha256", Some("sentry-hook-signature"))),
            ..Seeded::new(TriggerKind::Webhook, "sentry")
        },
    )
    .await;
    seed(
        &f.db,
        &fleet_id,
        &f.workspace,
        Seeded {
            channel: Some("C0123456789"),
            ..Seeded::new(TriggerKind::Mention, "slack")
        },
    )
    .await;
    let path = format!("{}/triggers", f.fleet(&fleet));

    let listed = send(&router, Method::GET, &path, Some(&f.token), "").await;
    assert_eq!(listed.status(), StatusCode::OK);
    let listed = json_body(listed).await;
    let items = listed["items"].as_array().expect("the triggers");
    assert_eq!(items.len(), 2);
    let webhook = items
        .iter()
        .find(|item| item["kind"] == "webhook")
        .expect("the webhook row");
    assert!(
        webhook["url"]
            .as_str()
            .is_some_and(|url| url.ends_with(&format!("/v1/webhooks/{fleet}/sentry"))),
        "{webhook}"
    );
    let mention = items
        .iter()
        .find(|item| item["kind"] == "mention")
        .expect("the mention row");
    assert_eq!(mention["channel"], "C0123456789");
    assert_eq!(mention["url"], Value::Null);
    assert!(!listed.to_string().contains("sentry-hook"), "{listed}");

    let item = format!("{path}/{hook}");
    for _ in 0..2 {
        let removed = send(&router, Method::DELETE, &item, Some(&f.token), "").await;
        assert_eq!(removed.status(), StatusCode::NO_CONTENT);
    }
    let after = json_body(send(&router, Method::GET, &path, Some(&f.token), "").await).await;
    assert_eq!(after["items"].as_array().map(Vec::len), Some(1));
    assert_eq!(after["items"][0]["kind"], json!("mention"));
    f.cleanup().await;
}
