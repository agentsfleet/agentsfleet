//! A channel no fleet is attached to answers through its resident.
//!
//! Split from the route cases beside it. The router here holds a live fleet
//! queue, because the first mention in a channel installs the resident and an
//! install creates the fleet's stream. What is read back is the store: the
//! fleet rows, the binding rows, the admissions, and the fleet's memory.

#![cfg(feature = "test-util")]

use std::borrow::Cow;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_fleet::memory::Memories;
use afd_ingress::slack::KIND_RESIDENT;
use afd_wire::memory::MemoryDelta;

use super::fixture::Admitted;
use super::resident_bound::{resident_document, resident_name};
use super::*;

/// A second channel in the same team, with no fleet attached either.
const OTHER_CHANNEL: &str = "C0987654321";
/// A second thread in [`CHANNEL`].
const SECOND_THREAD: &str = "1700000000.000300";
/// What every resident's name opens with.
const RESIDENT_PREFIX: &str = "slack-channel-";

/// One `app_mention` in `channel`, in the thread rooted at `thread_ts`.
fn mention_at(team: &str, event_id: &str, channel: &str, thread_ts: &str) -> String {
    let asked = format!("<@{BOT_USER}> who owns the deploy?");
    app_mention(team, event_id, PERSON, &asked, channel, thread_ts)
}

/// The resident fleets this workspace holds, by name.
async fn residents(fixture: &Fixture) -> Vec<String> {
    let mut connection = fixture.database().acquire().await.expect("a connection");
    sqlx::query_scalar(
        "SELECT name FROM core.fleets WHERE workspace_id = $1::uuid AND name LIKE $2 || '%' \
         ORDER BY name",
    )
    .bind(fixture.workspace().as_str())
    .bind(RESIDENT_PREFIX)
    .fetch_all(&mut *connection)
    .await
    .expect("the fleets read")
}

/// How many resident bindings `channel` holds in the fixture's team.
async fn bindings(fixture: &Fixture, channel: &str) -> i64 {
    let mut connection = fixture.database().acquire().await.expect("a connection");
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM core.connector_channels \
         WHERE provider = $1 AND external_account_id = $2 AND external_channel_id = $3 \
           AND kind = $4",
    )
    .bind(PROVIDER.id())
    .bind(&fixture.team)
    .bind(channel)
    .bind(KIND_RESIDENT)
    .fetch_one(&mut *connection)
    .await
    .expect("the bindings count")
}

/// The admission `event_id` produced, which must exist.
async fn admitted(fixture: &Fixture, event_id: &str) -> Admitted {
    fixture
        .admission(&key(&fixture.team, event_id))
        .await
        .expect("the mention was admitted")
}

/// Dimension 5.1 — two first mentions in one channel, delivered at once,
/// leave one resident fleet and one binding, and both are admitted on it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn concurrent_first_mentions_make_one_resident() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let router = fixture.resident_router().await;

    let deliveries = [("EvRes01", THREAD_TS), ("EvRes02", SECOND_THREAD)].map(|(event, thread)| {
        let router = router.clone();
        let body = mention_at(&fixture.team, event, CHANNEL, thread);
        tokio::spawn(async move { deliver(&router, &body).await.status() })
    });
    for delivery in deliveries {
        assert_eq!(
            delivery.await.expect("the delivery completes"),
            StatusCode::OK
        );
    }

    assert_eq!(residents(&fixture).await.len(), 1, "one resident fleet");
    assert_eq!(bindings(&fixture, CHANNEL).await, 1, "one binding");
    let first = admitted(&fixture, "EvRes01").await;
    let second = admitted(&fixture, "EvRes02").await;
    assert_eq!(
        first.fleet, second.fleet,
        "both answered by the one resident"
    );
    let request: Value = serde_json::from_str(&first.request_json).expect("the body is JSON");
    assert_eq!(
        request.pointer("/route/verdict").and_then(Value::as_str),
        Some("resident")
    );

    fixture.cleanup().await;
}

/// The install that loses the race for the resident's name converges on the
/// fleet that won it: that fleet is found by name, bound, and answers.
///
/// Arranged rather than raced, so the losing branch runs every time: the
/// resident's fleet already exists under its name with no binding yet, which
/// is the state a concurrent first mention leaves between its install and its
/// bind.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_first_mention_that_loses_the_name_converges_on_the_winner() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let name = resident_name(&fixture);
    let winner = fixture
        .fleet(&resident_document(&name), FleetStatus::Active.as_str())
        .await;
    let router = fixture.resident_router().await;

    let body = mention_at(&fixture.team, "EvRes03", CHANNEL, THREAD_TS);
    assert_eq!(deliver(&router, &body).await.status(), StatusCode::OK);

    assert_eq!(residents(&fixture).await, [name], "no second resident");
    assert_eq!(bindings(&fixture, CHANNEL).await, 1, "the winner is bound");
    assert_eq!(admitted(&fixture, "EvRes03").await.fleet, winner.as_str());

    fixture.cleanup().await;
}

/// Dimension 5.3 — the resident's memory is its channel's: every thread in a
/// channel reaches one resident, a fact it captured is there for the next
/// thread, and another channel's resident does not read it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn resident_memory_is_the_channel() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let router = fixture.resident_router().await;

    for (event, channel, thread) in [
        ("EvMem01", CHANNEL, THREAD_TS),
        ("EvMem02", CHANNEL, SECOND_THREAD),
        ("EvMem03", OTHER_CHANNEL, THREAD_TS),
    ] {
        let body = mention_at(&fixture.team, event, channel, thread);
        assert_eq!(deliver(&router, &body).await.status(), StatusCode::OK);
    }
    let first_thread = admitted(&fixture, "EvMem01").await.fleet;
    let second_thread = admitted(&fixture, "EvMem02").await.fleet;
    let other_channel = admitted(&fixture, "EvMem03").await.fleet;
    assert_eq!(first_thread, second_thread, "one channel, one resident");
    assert_ne!(first_thread, other_channel, "one resident per channel");

    let memories = Memories::new(fixture.database(), Entropy::new());
    let channel = Uuid7::parse(&first_thread).expect("a fleet id");
    let elsewhere = Uuid7::parse(&other_channel).expect("a fleet id");
    let fact = MemoryDelta {
        key: Cow::Borrowed("deploy-owner"),
        content: Cow::Borrowed("The platform team owns deploys."),
        category: Cow::Borrowed("core"),
    };
    memories
        .capture(
            &channel,
            &[fact],
            UnixMillis::from_millis(1_700_000_000_000),
        )
        .await
        .expect("the resident captures a fact");

    let recalled = memories
        .list(&channel)
        .await
        .expect("the channel's memory reads");
    assert!(
        recalled.iter().any(|entry| entry.key == "deploy-owner"),
        "the next thread in the channel recalls it"
    );
    assert!(
        memories
            .list(&elsewhere)
            .await
            .expect("the other channel's memory reads")
            .is_empty(),
        "another channel's resident does not"
    );

    fixture.cleanup().await;
}

/// A resident whose install fails for any reason but a taken name refuses the
/// mention with the install's own answer, and leaves no fleet or binding
/// behind. Here the fleet's stream cannot be created, because this router's
/// queue is not there, so the install rolls back: the lifecycle's 500
/// (`InstallRolledBack`), which Slack retries like any 5xx.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_resident_that_cannot_be_installed_refuses_for_a_retry() {
    let fixture = Fixture::create().await;
    fixture.seed().await;

    let body = mention_at(&fixture.team, "EvRes20", CHANNEL, THREAD_TS);
    let status = deliver(&fixture.router(), &body).await.status();

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(
        residents(&fixture).await.is_empty(),
        "the install rolled back"
    );
    assert_eq!(bindings(&fixture, CHANNEL).await, 0, "nothing was bound");
    assert!(
        fixture
            .admission(&key(&fixture.team, "EvRes20"))
            .await
            .is_none(),
        "nothing was admitted"
    );

    fixture.cleanup().await;
}

/// The install lost the name, and the read that follows finds nothing under
/// it: the winner was deleted between the two. No fleet can answer, so the
/// mention is refused as an outage and Slack's retry starts over.
///
/// Scripted rather than raced: the ingress answers the name read with
/// nothing, while the name really is taken in the live fleets store.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_name_gone_after_the_race_refuses_for_a_retry() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    fixture
        .fleet(
            &resident_document(&resident_name(&fixture)),
            FleetStatus::Active.as_str(),
        )
        .await;
    let ingress =
        std::sync::Arc::new(harness::Scripted::new().installed_in(fixture.workspace().clone()));

    let body = mention_at(&fixture.team, "EvRes21", CHANNEL, THREAD_TS);
    let status = deliver(&fixture.scripted_router(&ingress), &body)
        .await
        .status();

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(ingress.deliveries().is_empty(), "nothing was admitted");

    fixture.cleanup().await;
}
