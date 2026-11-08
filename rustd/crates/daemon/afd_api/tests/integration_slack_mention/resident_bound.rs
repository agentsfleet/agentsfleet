//! Existing notice carriers are workspace scoped and never become a fallback runner.
#![cfg(feature = "test-util")]

use super::notice::owed;
use super::*;
use af_core::id::Uuid7;
use afd_db::test_util::mint_id;
use afd_ingress::slack::{KIND_NOTICE_OWNER, notice_key};

pub(super) fn owner_document() -> String {
    document("notice-carrier", CHANNEL, None)
}

pub(super) async fn bind(fixture: &Fixture, fleet: &Uuid7) {
    let mut connection = fixture.database().acquire().await.expect("connection");
    sqlx::query("INSERT INTO connector.channels (id, provider, external_account_id, external_channel_id, fleet_id, binding_type, created_at, updated_at) VALUES ($1::uuid,$2,$3,$4,$5::uuid,$6,now(),now())")
        .bind(mint_id()).bind(PROVIDER.id()).bind(&fixture.team).bind(CHANNEL)
        .bind(fleet.as_str()).bind(KIND_NOTICE_OWNER)
        .execute(&mut *connection).await.expect("binding seeds");
}

pub(super) async fn bound(fixture: &Fixture) -> (String, String) {
    let mut connection = fixture.database().acquire().await.expect("connection");
    sqlx::query_as("SELECT c.fleet_id::text, f.workspace_id::text FROM connector.channels c JOIN fleet.fleets f ON f.id=c.fleet_id WHERE c.provider=$1 AND c.external_account_id=$2 AND c.external_channel_id=$3")
        .bind(PROVIDER.id()).bind(&fixture.team).bind(CHANNEL)
        .fetch_one(&mut *connection).await.expect("channel binding")
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_moved_team_uses_its_current_workspaces_notice_carrier() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let old = fixture
        .fleet_elsewhere(&owner_document(), FleetStatus::Active.as_str())
        .await;
    bind(&fixture, &old).await;
    let first = fixture
        .fleet(
            &document("responder", CHANNEL, None),
            FleetStatus::Active.as_str(),
        )
        .await;
    fixture
        .fleet(
            &document("triage", CHANNEL, None),
            FleetStatus::Active.as_str(),
        )
        .await;
    let body = mention(
        &fixture.team,
        "EvMoved01",
        PERSON,
        &format!("<@{BOT_USER}> who is on call?"),
    );
    assert_eq!(
        deliver(&fixture.router(), &body).await.status(),
        StatusCode::OK
    );
    assert_eq!(
        bound(&fixture).await,
        (first.to_string(), fixture.workspace().to_string())
    );
    assert_eq!(fixture.admissions().await, 0);
    let notices = owed(&fixture, &notice_key(&fixture.team, "EvMoved01")).await;
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].owner, "responder");
    fixture.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_stopped_subscriber_owes_the_paused_notice_without_admission() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let stopped = fixture
        .fleet(&owner_document(), FleetStatus::Stopped.as_str())
        .await;
    bind(&fixture, &stopped).await;
    let body = mention(
        &fixture.team,
        "EvStopped01",
        PERSON,
        &format!("<@{BOT_USER}> notice-carrier: who is on call?"),
    );
    assert_eq!(
        deliver(&fixture.router(), &body).await.status(),
        StatusCode::OK
    );
    assert_eq!(fixture.admissions().await, 0);
    let notices = owed(&fixture, &notice_key(&fixture.team, "EvStopped01")).await;
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].owner, "notice-carrier");
    assert!(
        notices[0]
            .answer
            .contains(&format!("agentsfleet resume {}", stopped.as_str()))
    );
    fixture.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_killed_bound_carrier_drops_notices_even_with_active_subscribers() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    for name in ["responder", "triage"] {
        fixture
            .fleet(&document(name, CHANNEL, None), FleetStatus::Active.as_str())
            .await;
    }
    let killed = fixture
        .fleet(&owner_document(), FleetStatus::Killed.as_str())
        .await;
    bind(&fixture, &killed).await;
    let body = mention(
        &fixture.team,
        "EvKilled02",
        PERSON,
        &format!("<@{BOT_USER}> who is on call?"),
    );
    let answer = json_body(deliver(&fixture.router(), &body).await).await;
    assert_eq!(
        answer.get("ignored").and_then(Value::as_str),
        Some("notice_owner_killed")
    );
    assert_eq!(fixture.admissions().await, 0);
    assert!(
        owed(&fixture, &notice_key(&fixture.team, "EvKilled02"))
            .await
            .is_empty()
    );
    assert_eq!(bound(&fixture).await.0, killed.as_str());
    fixture.cleanup().await;
}
