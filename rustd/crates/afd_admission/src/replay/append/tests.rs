//! A pass whose row its inserter receipted while the pass appended.
//!
//! Driven through `record_replays` directly rather than through a whole pass:
//! a pass claims every owed row in the shared lane, so which pass meets the
//! race could not be chosen from outside, and the branch is about what the
//! RECORD does with a row it did not receipt.
#![expect(
    clippy::expect_used,
    reason = "test: an unmet precondition should fail the test loudly"
)]

use afd_core::test_util::trace::Capture;
use afd_db::DbRole;
use afd_db::test_util::{FleetSeed, TenantSeed, TestDatabase, WorkspaceSeed, mint_id};
use afd_dragonfly::config::{DragonflyConfig, DragonflyRole};
use afd_dragonfly::{Dragonfly, EventId};
use afd_wire::event::EventType;

use super::super::Pending;
use super::super::pass::Drained;
use crate::{Admission, Admissions, Key, Producer, Reply};

/// The receipt the inserter recorded first.
const INSERTER_RECEIPT: &str = "1790000000000-9";

/// The receipt the pass's own append answered.
const PASS_RECEIPT: &str = "1790000000000-0";

/// What the claimed row is read back as, by the pass's own column names.
const SELECT_PENDING: &str = "SELECT id::text AS id, fleet_id::text AS fleet_id, \
     workspace_id::text AS workspace_id, actor, event_type, request_json, event_created_at \
     FROM fleet.admissions WHERE id = $1::uuid";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_row_its_inserter_receipted_mid_pass_is_released_not_recorded() {
    let lane = TestDatabase::shared();
    let database = lane.open(DbRole::Api, &[]).await;
    let (tenant, workspace, fleet) = (mint_id(), mint_id(), mint_id());
    let mut connection = database.acquire().await.expect("a pooled connection");
    TenantSeed::new(&tenant)
        .insert(&mut *connection)
        .await
        .expect("seeding a tenant");
    WorkspaceSeed::new(&workspace, &tenant)
        .insert(&mut *connection)
        .await
        .expect("seeding a workspace");
    FleetSeed::new(&fleet, &workspace, &tenant, "stopped")
        .name("superseded-replay-fleet")
        .native_setup()
        .insert(&mut *connection)
        .await
        .expect("seeding a fleet");
    // Only Postgres is touched: the append already happened, as `PASS_RECEIPT`.
    let queue = Dragonfly::unreachable(&DragonflyConfig::from_url(
        DragonflyRole::Default,
        "redis://127.0.0.1:1".to_owned(),
    ))
    .expect("a handle over nothing");
    let admissions = Admissions::for_tests(database.clone(), queue);
    let row = afd_core::id::Uuid7::parse(&mint_id()).expect("a minted id");
    let admission = Admission {
        producer: Producer::Steer,
        key: Key::Repeated(&fleet),
        fleet: &fleet,
        workspace: &workspace,
        actor: "fixture:superseded",
        event_type: EventType::Chat,
        request_json: r#"{"message":"superseded"}"#,
        reply: Reply::None,
    };
    admissions
        .insert_on(
            &mut connection,
            &row,
            &admission,
            &fleet,
            &admission.payload_digest(),
            0,
        )
        .await
        .expect("the row is admitted");
    // Claimed by this pass, and receipted by its inserter while it appended.
    sqlx::query(
        "UPDATE fleet.admissions SET receipt = $2, \
           replay_claim_expires_at = now() + interval '1 hour' WHERE id = $1::uuid",
    )
    .bind(row.as_str())
    .bind(INSERTER_RECEIPT)
    .execute(&mut *connection)
    .await
    .expect("the race is staged");
    let pending: Pending = sqlx::query_as(SELECT_PENDING)
        .bind(row.as_str())
        .fetch_one(&mut *connection)
        .await
        .expect("the claimed row reads back");
    let drained = Drained {
        appended: vec![(&pending, EventId::of(PASS_RECEIPT))],
        unreached: &[],
    };

    let capture = Capture::install();
    admissions
        .record_replays(&drained)
        .await
        .expect("the record and the release run");

    let logged = capture.only("admission_replay_superseded");
    assert_eq!(logged.field("event_id"), Some(row.as_str()));
    assert_eq!(logged.field("receipt"), Some(PASS_RECEIPT));
    let (receipt, claimed, replays): (Option<String>, bool, i64) = sqlx::query_as(
        "SELECT receipt, replay_claim_expires_at IS NOT NULL, replay_count \
           FROM fleet.admissions WHERE id = $1::uuid",
    )
    .bind(row.as_str())
    .fetch_one(&mut *connection)
    .await
    .expect("the row reads back");
    assert_eq!(receipt.as_deref(), Some(INSERTER_RECEIPT), "the first receipt stands");
    assert!(!claimed, "the pass's claim is released");
    assert_eq!(replays, 0, "a row the pass did not receipt is not counted as replayed");

    sqlx::query("DELETE FROM fleet.fleets WHERE id = $1::uuid")
        .bind(&fleet)
        .execute(&mut *connection)
        .await
        .expect("the fleet and its admissions go");
    for (statement, id) in [
        ("DELETE FROM identity.workspaces WHERE id = $1::uuid", &workspace),
        ("DELETE FROM identity.tenants WHERE id = $1::uuid", &tenant),
    ] {
        sqlx::query(statement)
            .bind(id)
            .execute(&mut *connection)
            .await
            .expect("fixture rows go");
    }
    drop(connection);
    lane.cleanup().await;
}
