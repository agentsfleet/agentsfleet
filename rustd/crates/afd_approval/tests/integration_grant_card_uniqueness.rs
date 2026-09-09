//! Slot 836: one OPEN grant card per (fleet, service), held by the database.
//!
//! # Why this needs its own file and a real datastore
//!
//! `REQUEST_GRANT`'s `NOT EXISTS` guard means no SERIAL caller ever reaches the
//! `ON CONFLICT` clause — the guard answers first, every time. So the seven
//! tests beside this one exercise the statement and never once exercise the
//! constraint that makes it safe under concurrency. What they do prove, for
//! free, is that the arbiter is INFERABLE: PostgreSQL refuses an `ON CONFLICT`
//! whose specification matches no index at plan time, so a missing or misspelled
//! slot 836 fails all of them rather than passing quietly.
//!
//! This file proves the other half — that the index actually REFUSES the row —
//! by writing the card directly, which is the only way past a guard reading its
//! own snapshot. Two concurrent transactions would prove the same thing and
//! prove it flakily; a direct insert is the same collision with a deterministic
//! schedule.
//!
//! # And why the shape is asserted, not just the collision
//!
//! A partial expression index has three ways to be wrong and only one to be
//! right: too wide (per-fleet, so a second service cannot be asked about), too
//! narrow (missing the predicate, so answered history collides with a new
//! question), or absent. The collision alone distinguishes none of them, so each
//! is a case below.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_approval::{IntegrationGrants, KIND_INTEGRATION_GRANT, Origin, Wanted};
use afd_crypto::entropy::Entropy;
use afd_wire::approval::status as gate_status;

use crate::lane::{Lane, NOW_MS, mint};

/// The service the lane's first, statement-written card names.
const SERVICE: &str = "github";

/// The index slot 836 installs, as PostgreSQL reports it on a collision.
const INDEX: &str = "uq_fleet_approval_gates_fleet_id_grant_service_pending";

/// Raises the one real card, through the statement under test.
async fn raise_first_card(lane: &Lane) {
    IntegrationGrants::new(lane.pool.clone(), Entropy::new())
        .request(
            &lane.workspace,
            &lane.fleet,
            Wanted {
                service: SERVICE,
                credential: "gh",
                origin: Origin::Install,
            },
            Lane::now(),
        )
        .await
        .expect("the first request must land");
}

/// Writes a card straight into the table, bypassing the statement's guard.
///
/// The column list is `REQUEST_GRANT`'s own, so a row that collides here is the
/// row the statement would have written and not a thinner fixture the index
/// might treat differently.
async fn insert_card(lane: &Lane, service: &str, status: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO core.fleet_approval_gates
           (id, fleet_id, workspace_id, action_id, tool_name, action_name,
            gate_kind, proposed_action, evidence, blast_radius, timeout_at,
            resolved_by, status, detail, created_at, event_id, stated_binding,
            spend_count, spend_ceiling)
         VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, $6,
                 $7, $8, $9::jsonb, $10, $11,
                 '', $12, '', $13, NULL, NULL, NULL, NULL)",
    )
    .bind(mint().as_str())
    .bind(lane.fleet.as_str())
    .bind(lane.workspace.as_str())
    .bind(mint().as_str())
    .bind(service)
    .bind("grant")
    .bind(KIND_INTEGRATION_GRANT)
    .bind(format!("mint short-lived credentials for {service}"))
    .bind(format!("{{\"service\":\"{service}\"}}"))
    .bind("every credential this fleet mints")
    .bind(NOW_MS)
    .bind(status)
    .bind(NOW_MS)
    .execute(&mut *lane.pool.acquire().await.expect("the lane must answer"))
    .await
    .map(|_| ())
}

/// The name of the constraint a failed insert collided with.
fn collided_with(outcome: Result<(), sqlx::Error>) -> Option<String> {
    outcome.err().and_then(|failure| {
        failure
            .as_database_error()
            .and_then(sqlx::error::DatabaseError::constraint)
            .map(str::to_owned)
    })
}

/// A second OPEN card for the same service is refused by the database.
///
/// The headline, and the guarantee `REQUEST_GRANT`'s doc comment now claims. Two
/// deliveries asking in the same instant both pass a snapshot-scoped
/// `NOT EXISTS`; this is what stops both from landing.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_second_open_card_for_one_service_cannot_be_written() {
    let lane = Lane::isolated().await;
    raise_first_card(&lane).await;

    let duplicate = insert_card(&lane, SERVICE, gate_status::PENDING).await;

    assert_eq!(
        collided_with(duplicate).as_deref(),
        Some(INDEX),
        "a second pending card for one service must collide with slot 836"
    );
}

/// The index is scoped per SERVICE, not per fleet.
///
/// The too-wide failure. A fleet declaring two mintable credentials must be able
/// to carry an open question about each; an index on `fleet_id` alone would let
/// the first card silence the second for ever.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_card_for_a_different_service_is_still_allowed() {
    let lane = Lane::isolated().await;
    raise_first_card(&lane).await;

    insert_card(&lane, "zoho", gate_status::PENDING)
        .await
        .expect("a different service is a different question");
}

/// The index covers only PENDING rows.
///
/// The too-narrow failure. Answered cards are the history the inbox exists to
/// show, and a resolve or a sweep moves a row out of the predicate — which is
/// precisely what lets the next request raise a fresh card for a grant that is
/// still pending.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_answered_card_does_not_block_the_next_question() {
    let lane = Lane::isolated().await;
    raise_first_card(&lane).await;

    for answered in [
        gate_status::APPROVED,
        gate_status::DENIED,
        gate_status::TIMED_OUT,
    ] {
        let collision = collided_with(insert_card(&lane, SERVICE, answered).await);
        assert!(
            collision.is_none(),
            "an answered card must not collide, hit {collision:?} for {answered}"
        );
    }
}
