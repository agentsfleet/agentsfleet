//! What the detail read counts beside the row: the approvals a human still
//! owes the fleet an answer on.
//!
//! One statement, proven against live Postgres because the count is a
//! correlated subselect over another table — a stub would prove the read
//! CALLS something, not that the predicate counts pending rows and only those.
//! `#[ignore]`d; `make test-integration-rustd` runs it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::id::Uuid7;
use afd_db::test_util::mint_id;

use crate::integration_patch_visibility::installed;
use crate::support::Lane;

/// The gate kind a seeded row carries; any served spelling will do.
const KIND: &str = "repository_write";

/// A minted identifier the gate table's version check accepts.
fn id() -> String {
    Uuid7::parse(&mint_id())
        .expect("a minted identifier is well formed")
        .as_str()
        .to_owned()
}

/// Seeds one gate on `fleet` in `status`, answering its action id.
async fn seed_gate(lane: &Lane, fleet: &Uuid7, status: &str) -> String {
    let action = mint_id();
    sqlx::query(
        "INSERT INTO core.fleet_approval_gates
           (id, fleet_id, workspace_id, action_id, tool_name, action_name,
            gate_kind, proposed_action, evidence, blast_radius, timeout_at,
            resolved_by, status, detail, created_at, updated_at, event_id,
            spend_count, spend_ceiling)
         VALUES ($1::uuid, $2::uuid, $3::uuid, $4, 'git', 'push',
                 $5, 'open a pull request', '{}'::jsonb, 'one repository',
                 $6, '', $7, '', $6, NULL, $8, 0, 32)",
    )
    .bind(id())
    .bind(fleet.as_str())
    .bind(lane.workspace.as_str())
    .bind(&action)
    .bind(KIND)
    .bind(Lane::now().as_millis())
    .bind(status)
    .bind(mint_id())
    .execute(&mut *lane.pool.acquire().await.expect("a pooled connection"))
    .await
    .expect("the gate row must insert");
    action
}

/// The count is the fleet's PENDING gates and nothing else.
///
/// A resolved gate on the same fleet and a pending gate on another fleet are
/// both seeded, because each is a predicate the statement could get wrong:
/// counting by fleet alone would show the answered one, counting by status
/// alone would show the neighbour's.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres and Redis"]
async fn the_detail_counts_the_fleets_pending_gates_and_only_those() {
    let lane = Lane::create().await;
    let fleet = installed(&lane).await;
    let neighbour = installed(&lane).await;

    assert_eq!(
        lane.fleets
            .detail(&lane.workspace, &fleet.id)
            .await
            .expect("the fleet reads back")
            .pending_approvals,
        0,
        "a fleet nobody has asked about owes no answer"
    );

    seed_gate(&lane, &fleet.id, "pending").await;
    seed_gate(&lane, &fleet.id, "pending").await;
    seed_gate(&lane, &fleet.id, "denied").await;
    seed_gate(&lane, &neighbour.id, "pending").await;

    assert_eq!(
        lane.fleets
            .detail(&lane.workspace, &fleet.id)
            .await
            .expect("the fleet reads back")
            .pending_approvals,
        2,
        "two pending; the answered one and the neighbour's are not this fleet's debt"
    );
    assert_eq!(
        lane.fleets
            .detail(&lane.workspace, &neighbour.id)
            .await
            .expect("the neighbour reads back")
            .pending_approvals,
        1
    );

    lane.cleanup().await;
}
