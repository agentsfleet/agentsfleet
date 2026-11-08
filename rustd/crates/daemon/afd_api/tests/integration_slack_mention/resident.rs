//! Unconfigured mentions create no fleet; notices reuse an existing stable carrier.
#![cfg(feature = "test-util")]

use super::notice::owed;
use super::resident_bound::{bind, bound};
use super::*;
use afd_ingress::slack::notice_key;

async fn stored(fixture: &Fixture) -> Value {
    let mut connection = fixture.database().acquire().await.expect("observer");
    sqlx::query_scalar("SELECT jsonb_build_object('fleets', COALESCE((SELECT jsonb_agg(to_jsonb(f) ORDER BY f.id) FROM fleet.fleets f WHERE workspace_id=$1::uuid),'[]'::jsonb), 'revisions', COALESCE((SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM fleet.revisions r JOIN fleet.fleets f ON f.id=r.fleet_id WHERE f.workspace_id=$1::uuid),'[]'::jsonb))")
        .bind(fixture.workspace().as_str()).fetch_one(&mut *connection).await.expect("fleet snapshot")
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn unconfigured_mentions_do_not_create_fleets_or_admissions_or_notices() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    fixture
        .fleet(
            &document("release-review", "C0987654321", Some("write")),
            FleetStatus::Active.as_str(),
        )
        .await;
    let before = stored(&fixture).await;
    let body = mention(
        &fixture.team,
        "EvUnconfigured",
        PERSON,
        &format!("<@{BOT_USER}> help deploy"),
    );
    for _ in 0..2 {
        let response = json_body(deliver(&fixture.router(), &body).await).await;
        assert_eq!(
            response.get("ignored").and_then(Value::as_str),
            Some("channel_not_configured")
        );
    }
    assert_eq!(fixture.admissions().await, 0);
    assert!(
        owed(&fixture, &notice_key(&fixture.team, "EvUnconfigured"))
            .await
            .is_empty()
    );
    assert_eq!(stored(&fixture).await, before);
    let channel = CHANNEL.parse().expect("channel");
    assert!(
        fixture
            .ingress()
            .notice_owner(fixture.workspace(), PROVIDER.id(), &fixture.team, &channel)
            .await
            .expect("owner lookup")
            .is_none()
    );
    fixture.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn concurrent_notices_choose_one_existing_owner_without_changing_fleets() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let first = fixture
        .fleet(
            &document("responder", CHANNEL, Some("read")),
            FleetStatus::Active.as_str(),
        )
        .await;
    let second = fixture
        .fleet(
            &document("triage", CHANNEL, Some("read")),
            FleetStatus::Active.as_str(),
        )
        .await;
    assert!(
        first.as_str() < second.as_str(),
        "fixture order is explicit"
    );
    let before = stored(&fixture).await;
    let one = mention(
        &fixture.team,
        "EvConcurrent01",
        PERSON,
        &format!("<@{BOT_USER}> help"),
    );
    let two = mention_in(
        &fixture.team,
        "EvConcurrent02",
        PERSON,
        &format!("<@{BOT_USER}> help"),
        "1700000000.000300",
    );
    let router = fixture.router();
    let (one, two) = tokio::join!(deliver(&router, &one), deliver(&router, &two));
    assert_eq!(
        (one.status(), two.status()),
        (StatusCode::OK, StatusCode::OK)
    );
    concurrent_outcome(&fixture, first.as_str(), before).await;
    fixture.cleanup().await;
}

async fn concurrent_outcome(fixture: &Fixture, first: &str, before: Value) {
    for event in ["EvConcurrent01", "EvConcurrent02"] {
        let notices = owed(fixture, &notice_key(&fixture.team, event)).await;
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].owner, "responder");
    }
    assert_eq!(bound(fixture).await.0, first);
    assert_eq!(fixture.admissions().await, 0);
    assert_eq!(stored(fixture).await, before);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn prior_kind_binding_remains_owner_when_subscribers_change() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    fixture
        .fleet(
            &document("responder", CHANNEL, None),
            FleetStatus::Active.as_str(),
        )
        .await;
    let carrier = fixture
        .fleet(
            &document("notice-carrier", "C0987654321", None),
            FleetStatus::Active.as_str(),
        )
        .await;
    bind(&fixture, &carrier).await;
    let mut connection = fixture
        .database()
        .acquire()
        .await
        .expect("legacy binding fixture");
    sqlx::query("UPDATE connector.channels SET binding_type=$1 WHERE fleet_id=$2::uuid")
        .bind("resident")
        .bind(carrier.as_str())
        .execute(&mut *connection)
        .await
        .expect("historical kind");
    drop(connection);
    fixture
        .fleet(
            &document("triage", CHANNEL, None),
            FleetStatus::Active.as_str(),
        )
        .await;
    let before = stored(&fixture).await;
    let body = mention(
        &fixture.team,
        "EvStable01",
        PERSON,
        &format!("<@{BOT_USER}> help"),
    );
    assert_eq!(
        deliver(&fixture.router(), &body).await.status(),
        StatusCode::OK
    );
    let notices = owed(&fixture, &notice_key(&fixture.team, "EvStable01")).await;
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].owner, "notice-carrier");
    assert_eq!(bound(&fixture).await.0, carrier.as_str());
    assert_eq!(fixture.admissions().await, 0);
    assert_eq!(stored(&fixture).await, before);
    fixture.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn deleting_a_carrier_allows_the_next_notice_to_bind_a_current_subscriber() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let carrier = fixture
        .fleet(
            &document("notice-carrier", "C0987654321", None),
            FleetStatus::Active.as_str(),
        )
        .await;
    bind(&fixture, &carrier).await;
    let mut connection = fixture
        .database()
        .acquire()
        .await
        .expect("deletion fixture");
    sqlx::query("DELETE FROM fleet.fleets WHERE id=$1::uuid")
        .bind(carrier.as_str())
        .execute(&mut *connection)
        .await
        .expect("carrier deleted");
    drop(connection);
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
        "EvRebind01",
        PERSON,
        &format!("<@{BOT_USER}> help"),
    );
    assert_eq!(
        deliver(&fixture.router(), &body).await.status(),
        StatusCode::OK
    );
    assert_eq!(bound(&fixture).await.0, first.as_str());
    let notices = owed(&fixture, &notice_key(&fixture.team, "EvRebind01")).await;
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].owner, "responder");
    assert_eq!(fixture.admissions().await, 0);
    fixture.cleanup().await;
}
