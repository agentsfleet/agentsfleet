//! No per-fleet cap: a fleet admits however much it holds, and only the
//! deployment ceiling refuses (`M242_001` §1).

use super::{
    lane::Lane,
    ledger::{admit, steer},
    queue::over_fake,
};
use afd_admission::Reply as ReplyTo;
use afd_db::test_util::mint_id;

/// The per-fleet cap the ledger enforced before `M242_001`.
const OLD_FLEET_CAP: usize = 10_000;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_fleet_admits_past_ten_thousand_unsettled() {
    let lane = Lane::seed().await;
    let (_server, admissions) = over_fake(&lane, &[]).await;
    let first = admit(&admissions, steer(&lane, &lane.fleet, ReplyTo::None)).await;
    let ids: Vec<_> = (1..OLD_FLEET_CAP).map(|_| mint_id()).collect();
    sqlx::query("INSERT INTO fleet.admissions (id, fleet_id, workspace_id, producer, producer_key, payload_digest, actor, event_type, request_json, event_created_at, created_at, updated_at) SELECT seed.id::uuid, a.fleet_id, a.workspace_id, a.producer, seed.id, a.payload_digest, a.actor, a.event_type, a.request_json, now(), now(), now() FROM fleet.admissions a CROSS JOIN unnest($2::text[]) AS seed(id) WHERE a.id=$1::uuid")
        .bind(&first.stored.id).bind(&ids)
        .execute(&mut *lane.database.acquire().await.expect("database"))
        .await.expect("seed the old cap's worth of unsettled rows");
    let next = admit(&admissions, steer(&lane, &mint_id(), ReplyTo::None)).await;
    assert!(!next.replayed, "the next message is admitted, not refused");
    lane.cleanup().await;
}
