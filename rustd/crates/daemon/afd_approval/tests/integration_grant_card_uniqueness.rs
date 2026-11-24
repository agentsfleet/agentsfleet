//! Grant-card ownership, release, and rollback against the real schema.
#![expect(clippy::expect_used, reason = "test preconditions must fail loudly")]

use crate::grant_lane::{GITHUB, SENTRY, ask, grants, only_card};
use crate::lane::{Lane, NOW, mint};
use afd_approval::{Decision, GrantKind, Requested, Wanted};

const INDEX: &str = "uq_approval_gates_active_grant_id";

/// Every card on the lane's fleet, and how many still hold their grant.
async fn counts(lane: &Lane) -> (i64, i64) {
    sqlx::query_as(
        "SELECT COUNT(*), COUNT(active_grant_id) FROM fleet.approval_gates WHERE fleet_id = $1",
    )
    .bind(&lane.fleet)
    .fetch_one(&mut *lane.pool.acquire().await.expect("connection"))
    .await
    .expect("card counts")
}

/// Two names, two grants, two cards: the active reference is per grant.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn each_name_holds_its_own_active_card() {
    let lane = Lane::isolated().await;
    let turn = lane.asking_turn().await;
    let first = ask(&lane, SENTRY, GrantKind::Secret, None, &turn).await;
    let second = ask(&lane, GITHUB, GrantKind::Integration, None, &turn).await;
    assert!(matches!(first, Requested::Raised { .. }), "{first:?}");
    assert!(matches!(second, Requested::Raised { .. }), "{second:?}");
    assert_eq!(counts(&lane).await, (2, 2));
}

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_second_active_reference_is_refused_by_the_database() {
    let lane = Lane::isolated().await;
    let turn = lane.asking_turn().await;
    ask(&lane, SENTRY, GrantKind::Secret, None, &turn).await;
    let failure = sqlx::query(
        "INSERT INTO fleet.approval_gates
        (id, fleet_id, workspace_id, action_id, tool_name, action_name, gate_type,
         proposed_action, evidence, blast_radius, expires_at, status,
         detail, created_at, updated_at, turn_id, active_grant_id)
        SELECT $2, fleet_id, workspace_id, $3, tool_name, action_name, gate_type,
         proposed_action, evidence, blast_radius, expires_at, status,
         detail, created_at, updated_at, turn_id, active_grant_id
        FROM fleet.approval_gates WHERE fleet_id = $1",
    )
    .bind(&lane.fleet)
    .bind(mint())
    .bind(mint())
    .execute(&mut *lane.pool.acquire().await.expect("connection"))
    .await
    .expect_err("duplicate active reference must collide");
    assert_eq!(
        failure
            .as_database_error()
            .and_then(sqlx::error::DatabaseError::constraint),
        Some(INDEX)
    );
}

/// An answer releases the active reference and keeps the card as history; a
/// later ask after an approval is covered, and after a denial raises anew.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn resolutions_release_the_active_reference_and_keep_history() {
    for (decision, after) in [(Decision::Approved, (1, 0)), (Decision::Denied, (2, 1))] {
        let lane = Lane::isolated().await;
        let turn = lane.asking_turn().await;
        ask(&lane, SENTRY, GrantKind::Secret, None, &turn).await;
        let card = only_card(&lane).await;
        lane.inbox
            .resolve(
                card.action.as_str(),
                decision,
                "fixture",
                "",
                Some(lane.fleet.as_str()),
            )
            .await
            .expect("resolution");
        assert_eq!(counts(&lane).await, (1, 0), "{decision:?}");

        let again = lane.asking_turn().await;
        let outcome = ask(&lane, SENTRY, GrantKind::Secret, None, &again).await;

        let expected_raise = decision == Decision::Denied;
        assert_eq!(
            matches!(outcome, Requested::Raised { .. }),
            expected_raise,
            "{decision:?}: {outcome:?}"
        );
        assert_eq!(counts(&lane).await, after, "{decision:?}");
    }
}

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_card_insert_failure_rolls_back_the_new_grant() {
    let lane = Lane::isolated().await;
    let turn = lane.asking_turn().await;
    let result = grants(&lane)
        .request(
            &mint(),
            &lane.fleet,
            Wanted {
                name: SENTRY,
                kind: GrantKind::Secret,
                scope: None,
                turn: &turn,
            },
            NOW,
        )
        .await;
    assert!(
        result.is_err(),
        "nonexistent workspace must refuse the card"
    );
    assert_eq!(counts(&lane).await, (0, 0));
    let grants: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM connector.integration_grants WHERE fleet_id = $1")
            .bind(&lane.fleet)
            .fetch_one(&mut *lane.pool.acquire().await.expect("connection"))
            .await
            .expect("grant count");
    assert_eq!(grants, 0, "card failure must not leave an orphan grant");
}
