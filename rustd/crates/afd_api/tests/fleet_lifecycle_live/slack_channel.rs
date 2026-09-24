//! Attaching a fleet to a Slack channel at install, through the route, and the
//! attachment surviving the document's round trip through `fleet update`.
//!
//! The proof is the subscriber read a mention takes, run against the stored
//! row, so "attached" means what the mention path would find, not what a
//! string search of the document suggests.

#![cfg(feature = "test-util")]

use afd_admission::Admissions;
use afd_connector::Provider;
use afd_dragonfly::Dragonfly;
use afd_ingress::Ingress;
use afd_ingress::slack::ChannelId;

use super::*;

/// The channel the install attaches.
const CHANNEL: &str = "C0123456789";

/// The fleets a mention in [`CHANNEL`] would reach, as the mention path reads
/// them.
async fn subscribers(fixture: &Fixture) -> Vec<Uuid7> {
    let database = fixture.database.clone();
    let queue = Dragonfly::unreachable(&harness::unreachable_queue())
        .expect("a lazy manager opens no socket");
    let ingress = Ingress::new(
        database.clone(),
        harness::vault(database.clone()),
        Admissions::for_tests(database, queue),
    );
    let channel: ChannelId = CHANNEL.parse().expect("a well-formed channel");
    ingress
        .mention_subscribers(&fixture.workspace, Provider::Slack.id(), &channel)
        .await
        .expect("the subscriber read runs")
        .into_iter()
        .map(|subscriber| subscriber.fleet)
        .collect()
}

/// Dimension 2.4 — an install naming a Slack channel stores a `TRIGGER.md`
/// whose `mention` trigger names it, so a mention there reaches the fleet; and
/// the stored document sent back through `fleet update` keeps it attached.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn install_with_a_channel_writes_the_mention_trigger() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let router = Fleet::live(
        fixture.database.clone(),
        &fixture.subject,
        ScopeSet::from_scopes(&Scope::ALL),
    )
    .with_owned_workspace(fixture.workspace.clone())
    .with_fleet_queue(fixture.database.clone(), harness::connect_redis().await)
    .router();
    let workspace = format!("/v1/workspaces/{}", fixture.workspace.as_str());

    let installed = send(
        &router,
        Method::POST,
        &format!("{workspace}/fleets"),
        Some(&fixture.token),
        &serde_json::json!({
            "platform_library_id": fixture.library,
            "slack_channel_id": CHANNEL,
        })
        .to_string(),
    )
    .await;
    let status = installed.status();
    let installed = json_body(installed).await;
    assert_eq!(status, StatusCode::CREATED, "{installed}");
    let fleet = Uuid7::parse(
        installed
            .get("fleet_id")
            .and_then(Value::as_str)
            .expect("an install returns its fleet"),
    )
    .expect("the installed fleet id is canonical");
    assert_eq!(
        subscribers(&fixture).await,
        std::slice::from_ref(&fleet),
        "a mention in the channel reaches the fleet"
    );

    let item = format!("{workspace}/fleets/{}", fleet.as_str());
    let read = send(&router, Method::GET, &item, Some(&fixture.token), "").await;
    assert_eq!(read.status(), StatusCode::OK);
    let etag = read
        .headers()
        .get(header::ETAG)
        .and_then(|value| value.to_str().ok())
        .expect("fleet detail carries an etag")
        .to_owned();
    let stored = json_body(read)
        .await
        .get("trigger_markdown")
        .and_then(Value::as_str)
        .expect("the fleet stores its TRIGGER.md")
        .to_owned();
    assert!(stored.contains(CHANNEL), "{stored}");

    let updated = send_with_headers(
        &router,
        Method::PATCH,
        &item,
        Some(&fixture.token),
        &serde_json::json!({ "trigger_markdown": stored }).to_string(),
        &[(header::IF_MATCH, &etag)],
    )
    .await;
    let status = updated.status();
    assert_eq!(status, StatusCode::OK, "{}", json_body(updated).await);
    assert_eq!(
        subscribers(&fixture).await,
        [fleet],
        "the document sent back keeps the fleet attached"
    );

    fixture.cleanup().await;
}
