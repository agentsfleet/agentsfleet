//! Grant-card ownership, contention, and release against the real schema.
#![expect(clippy::expect_used, reason = "test preconditions must fail loudly")]

use crate::lane::{Lane, mint, sweeper_exclusive};
use afd_approval::{Decision, IntegrationGrants, Origin, Requested, Wanted};
use afd_crypto::entropy::Entropy;

const SERVICE: &str = "github";
const INDEX: &str = "uq_fleet_approval_gates_active_grant_id";

async fn request(lane: &Lane, service: &str) -> Requested {
    IntegrationGrants::new(lane.pool.clone(), Entropy::new())
        .request(
            &lane.workspace,
            &lane.fleet,
            Wanted {
                service,
                credential: service,
                origin: Origin::Park,
            },
            Lane::now(),
        )
        .await
        .expect("request runs")
}

async fn counts(lane: &Lane) -> (i64, i64) {
    sqlx::query_as("SELECT COUNT(*), COUNT(active_grant_id) FROM core.fleet_approval_gates WHERE fleet_id = $1::uuid")
        .bind(lane.fleet.as_str())
        .fetch_one(&mut *lane.pool.acquire().await.expect("connection"))
        .await.expect("card counts")
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn concurrent_requests_share_one_active_grant_card() {
    let lane = Lane::isolated().await;
    let (first, second) = tokio::join!(request(&lane, SERVICE), request(&lane, SERVICE));
    assert!(matches!(
        (first, second),
        (Requested::Raised, Requested::Pending) | (Requested::Pending, Requested::Raised)
    ));
    assert_eq!(counts(&lane).await, (1, 1));
    assert_eq!(request(&lane, "zoho").await, Requested::Raised);
    assert_eq!(counts(&lane).await, (2, 2));
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_second_active_reference_is_refused_by_the_database() {
    let lane = Lane::isolated().await;
    request(&lane, SERVICE).await;
    let failure = sqlx::query(
        "INSERT INTO core.fleet_approval_gates
        (id, fleet_id, workspace_id, action_id, tool_name, action_name, gate_kind,
         proposed_action, evidence, blast_radius, timeout_at, resolved_by, status,
         detail, created_at, active_grant_id)
        SELECT $2::uuid, fleet_id, workspace_id, $3, tool_name, action_name, gate_kind,
         proposed_action, evidence, blast_radius, timeout_at, resolved_by, status,
         detail, created_at, active_grant_id
        FROM core.fleet_approval_gates WHERE fleet_id = $1::uuid",
    )
    .bind(lane.fleet.as_str())
    .bind(mint().as_str())
    .bind(mint().as_str())
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

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn resolutions_release_the_active_reference_and_keep_history() {
    for decision in [Decision::Approved, Decision::Denied] {
        let lane = Lane::isolated().await;
        request(&lane, SERVICE).await;
        let action: String = sqlx::query_scalar(
            "SELECT action_id FROM core.fleet_approval_gates WHERE fleet_id = $1::uuid",
        )
        .bind(lane.fleet.as_str())
        .fetch_one(&mut *lane.pool.acquire().await.expect("connection"))
        .await
        .expect("card action");
        lane.inbox
            .resolve(
                &action,
                decision,
                "fixture",
                "",
                Some(lane.fleet.as_str()),
                Lane::now(),
            )
            .await
            .expect("resolution");
        assert_eq!(counts(&lane).await, (1, 0));
        assert!(matches!(
            request(&lane, SERVICE).await,
            Requested::Approved | Requested::Denied
        ));
        assert_eq!(counts(&lane).await, (1, 0));
    }
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn expiry_releases_the_reference_for_one_replacement_card() {
    let _sweeper = sweeper_exclusive().await;
    let lane = Lane::isolated().await;
    request(&lane, SERVICE).await;
    sqlx::query("UPDATE core.fleet_approval_gates SET timeout_at = $2 WHERE fleet_id = $1::uuid")
        .bind(lane.fleet.as_str())
        .bind(Lane::now().as_millis())
        .execute(&mut *lane.pool.acquire().await.expect("connection"))
        .await
        .expect("deadline fixture");
    lane.inbox.expire(Lane::now()).await.expect("expiry");
    assert_eq!(counts(&lane).await, (1, 0));
    assert_eq!(request(&lane, SERVICE).await, Requested::Raised);
    assert_eq!(request(&lane, SERVICE).await, Requested::Pending);
    assert_eq!(counts(&lane).await, (2, 1));
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_card_insert_failure_rolls_back_the_new_grant() {
    let lane = Lane::isolated().await;
    let result = IntegrationGrants::new(lane.pool.clone(), Entropy::new())
        .request(
            &mint(),
            &lane.fleet,
            Wanted {
                service: SERVICE,
                credential: SERVICE,
                origin: Origin::Park,
            },
            Lane::now(),
        )
        .await;
    assert!(
        result.is_err(),
        "nonexistent workspace must refuse the card"
    );
    assert_eq!(counts(&lane).await, (0, 0));
    let grants: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM core.integration_grants WHERE fleet_id = $1::uuid",
    )
    .bind(lane.fleet.as_str())
    .fetch_one(&mut *lane.pool.acquire().await.expect("connection"))
    .await
    .expect("grant count");
    assert_eq!(grants, 0, "card failure must not leave an orphan grant");
}
