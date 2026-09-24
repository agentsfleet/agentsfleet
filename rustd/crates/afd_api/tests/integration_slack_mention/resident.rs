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
    let name = format!(
        "{RESIDENT_PREFIX}{}-{}",
        fixture.team.to_ascii_lowercase(),
        CHANNEL.to_ascii_lowercase()
    );
    let winner = fixture
        .fleet(
            &format!(
                "---\nname: {name}\nx-agentsfleet:\n  triggers:\n    - type: api\n  tools: []\n  \
                 budget:\n    daily_dollars: 1.0\n---\n"
            ),
            FleetStatus::Active.as_str(),
        )
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
