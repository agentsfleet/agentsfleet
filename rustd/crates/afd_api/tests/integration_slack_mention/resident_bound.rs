//! A channel's resident is found in the mention's own workspace, and runs a
//! mention only when it can.
//!
//! Split from `resident.rs`, which installs one. Each case arranges a binding
//! a real deployment reaches: a Slack team that moved to another workspace,
//! and a resident an operator stopped or killed. What is read back is the
//! binding, the admissions, and the notice ledger.

#![cfg(feature = "test-util")]

use afd_core::id::Uuid7;
use afd_db::test_util::mint_id;
use afd_ingress::slack::{KIND_RESIDENT, notice_key};

use super::notice::owed;
use super::*;

/// What every resident's name opens with.
const RESIDENT_PREFIX: &str = "slack-channel-";

/// The name [`CHANNEL`]'s resident carries in `fixture`'s team.
pub(super) fn resident_name(fixture: &Fixture) -> String {
    format!(
        "{RESIDENT_PREFIX}{}-{}",
        fixture.team.to_ascii_lowercase(),
        CHANNEL.to_ascii_lowercase()
    )
}

/// A resident's document, as the daemon writes one.
pub(super) fn resident_document(name: &str) -> String {
    format!(
        "---\nname: {name}\nx-agentsfleet:\n  triggers:\n    - type: api\n  tools: []\n  \
         budget:\n    daily_dollars: 1.0\n---\n"
    )
}

/// An unaddressed question to the bot, which only the resident can take.
fn asking() -> String {
    format!("<@{BOT_USER}> who is on call?")
}

/// Binds `fleet` as [`CHANNEL`]'s resident, as an earlier first mention did.
async fn bind(fixture: &Fixture, fleet: &Uuid7) {
    let mut connection = fixture.database().acquire().await.expect("a connection");
    sqlx::query(
        "INSERT INTO core.connector_channels \
           (id, provider, external_account_id, external_channel_id, fleet_id, kind, created_at) \
         VALUES ($1::uuid, $2, $3, $4, $5::uuid, $6, 1)",
    )
    .bind(mint_id())
    .bind(PROVIDER.id())
    .bind(&fixture.team)
    .bind(CHANNEL)
    .bind(fleet.as_str())
    .bind(KIND_RESIDENT)
    .execute(&mut *connection)
    .await
    .expect("the binding seeds");
}

/// The fleet [`CHANNEL`]'s binding names now, with that fleet's workspace.
async fn bound(fixture: &Fixture) -> (String, String) {
    let mut connection = fixture.database().acquire().await.expect("a connection");
    sqlx::query_as(
        "SELECT c.fleet_id::text, f.workspace_id::text \
         FROM core.connector_channels c JOIN core.fleets f ON f.id = c.fleet_id \
         WHERE c.provider = $1 AND c.external_account_id = $2 AND c.external_channel_id = $3",
    )
    .bind(PROVIDER.id())
    .bind(&fixture.team)
    .bind(CHANNEL)
    .fetch_one(&mut *connection)
    .await
    .expect("one binding for the channel")
}

/// A Slack team that moved workspace left its resident's binding behind. The
/// next mention installs a resident in the workspace the team maps to now,
/// re-points the binding to it, and never runs the fleet it left.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_moved_team_never_runs_the_workspace_it_left() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let name = resident_name(&fixture);
    let left_behind = fixture
        .fleet_elsewhere(&resident_document(&name), FleetStatus::Active.as_str())
        .await;
    bind(&fixture, &left_behind).await;
    let router = fixture.resident_router().await;

    let body = mention(&fixture.team, "EvMoved01", PERSON, &asking());
    assert_eq!(deliver(&router, &body).await.status(), StatusCode::OK);

    let admitted = fixture
        .admission(&key(&fixture.team, "EvMoved01"))
        .await
        .expect("the mention was admitted");
    assert_ne!(
        admitted.fleet,
        left_behind.as_str(),
        "the other workspace's fleet never runs it"
    );
    let (fleet, workspace) = bound(&fixture).await;
    assert_eq!(fleet, admitted.fleet, "the binding follows the team");
    assert_eq!(workspace, fixture.workspace().as_str());

    fixture.cleanup().await;
}

/// A stopped resident answers with the paused notice and its resume command,
/// and nothing is admitted onto a fleet that would never run it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_stopped_resident_answers_with_the_paused_notice() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let name = resident_name(&fixture);
    let stopped = fixture
        .fleet(&resident_document(&name), FleetStatus::Stopped.as_str())
        .await;
    bind(&fixture, &stopped).await;
    let router = fixture.resident_router().await;

    let body = mention(&fixture.team, "EvStopped01", PERSON, &asking());
    assert_eq!(deliver(&router, &body).await.status(), StatusCode::OK);

    assert_eq!(
        fixture.admissions().await,
        0,
        "nothing runs on a stopped fleet"
    );
    let owed = owed(&fixture, &notice_key(&fixture.team, "EvStopped01")).await;
    let notice = owed.first().expect("the paused notice is owed");
    assert_eq!(owed.len(), 1);
    assert_eq!(notice.owner, name);
    assert!(
        notice
            .answer
            .contains(&format!("agentsfleet resume {}", stopped.as_str())),
        "the notice names the resume command: {}",
        notice.answer
    );

    fixture.cleanup().await;
}

/// A killed resident is on its way to deletion: its mention is dropped with
/// the reason, and neither an event nor a notice is written for it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_killed_resident_drops_the_mention() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let killed = fixture
        .fleet(
            &resident_document(&resident_name(&fixture)),
            FleetStatus::Killed.as_str(),
        )
        .await;
    bind(&fixture, &killed).await;
    let router = fixture.resident_router().await;

    let body = mention(&fixture.team, "EvKilled01", PERSON, &asking());
    let answered = json_body(deliver(&router, &body).await).await;

    assert_eq!(
        answered.get("ignored").and_then(Value::as_str),
        Some("resident_killed")
    );
    assert_eq!(fixture.admissions().await, 0);
    assert!(
        owed(&fixture, &notice_key(&fixture.team, "EvKilled01"))
            .await
            .is_empty(),
        "no notice is owed by a killed fleet"
    );

    fixture.cleanup().await;
}

/// A notice needs the channel's resident to owe it, so a killed resident drops
/// a mention that would have been answered with one, and owes nothing.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_killed_resident_owes_no_notice() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    for fleet in ["responder", "triage"] {
        fixture
            .fleet(
                &document(fleet, CHANNEL, Some("read")),
                FleetStatus::Active.as_str(),
            )
            .await;
    }
    let killed = fixture
        .fleet(
            &resident_document(&resident_name(&fixture)),
            FleetStatus::Killed.as_str(),
        )
        .await;
    bind(&fixture, &killed).await;
    let router = fixture.resident_router().await;

    let body = mention(&fixture.team, "EvKilled02", PERSON, &asking());
    let answered = json_body(deliver(&router, &body).await).await;

    assert_eq!(
        answered.get("ignored").and_then(Value::as_str),
        Some("resident_killed")
    );
    assert!(
        owed(&fixture, &notice_key(&fixture.team, "EvKilled02"))
            .await
            .is_empty()
    );

    fixture.cleanup().await;
}

/// A fleet somebody installed under the resident's name, with repository write
/// access of its own, is never adopted: the unnamed mention it would have
/// taken is dropped, and nothing is bound to the channel.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_fleet_holding_the_residents_name_is_never_adopted() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    // Attached elsewhere, so it is no subscriber of this channel and the
    // unnamed mention goes looking for the resident.
    fixture
        .fleet(
            &document(&resident_name(&fixture), "C0987654321", Some("write")),
            FleetStatus::Active.as_str(),
        )
        .await;
    let router = fixture.resident_router().await;

    let body = mention(&fixture.team, "EvSquat01", PERSON, &asking());
    let answered = json_body(deliver(&router, &body).await).await;

    assert_eq!(
        answered.get("ignored").and_then(Value::as_str),
        Some("resident_name_taken"),
        "{answered}"
    );
    assert_eq!(fixture.admissions().await, 0, "the squatter runs nothing");
    let mut connection = fixture.database().acquire().await.expect("a connection");
    let bindings: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM core.connector_channels \
         WHERE provider = $1 AND external_account_id = $2 AND external_channel_id = $3",
    )
    .bind(PROVIDER.id())
    .bind(&fixture.team)
    .bind(CHANNEL)
    .fetch_one(&mut *connection)
    .await
    .expect("the bindings count");
    drop(connection);
    assert_eq!(bindings, 0, "and is never bound as the resident");

    fixture.cleanup().await;
}
