//! A gate of any other type moves no grant.
//!
//! # The escalation this file exists to refuse
//!
//! `RESOLVE_GATE`'s `granted` arm decides whether answering a gate also moves
//! a fleet's standing permission, and it keys on the gate's type and the
//! `service` its evidence names. The one gate a fleet's run raises on its own
//! words is a question, whose text the model writes. So a run could ask a
//! question that reads like a harmless check while its card sits beside a
//! pending grant, and a person answering it would be answering about the
//! question, never the grant. The type is the discriminator: only the request
//! verb writes `integration_grant`, and every other type leaves the grant
//! exactly as it found it — even with the grant's own service in its evidence,
//! the most favourable row a forger could hope for.
//!
//! These need a live datastore because the claim is a claim about ONE
//! statement: that `granted` declines to move a row.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_api_wire::approval::GATE_TYPE_QUESTION;
use afd_api_wire::grant::status;
use afd_approval::{Decision, GrantKind};
use std::time::Duration;

use af_core::id::Uuid7;

use crate::grant_lane::{GITHUB, answer, ask, grant};
use crate::lane::{Lane, mint};

/// How long the question stands, from the database's clock.
const QUESTION_WINDOW: Duration = Duration::from_hours(1);

/// A question gate carrying the grant's service in its evidence.
const QUESTION_NAMING_THE_SERVICE: &str = "\
INSERT INTO fleet.approval_gates
  (id, fleet_id, workspace_id, action_id, tool_name, action_name,
   gate_type, proposed_action, evidence, blast_radius, expires_at,
   status, detail, created_at, updated_at, turn_id)
VALUES ($1, $2, $3, $4, $5, $5, $5, 'May I run the routine check?',
        $6::jsonb, '', now() + $7::interval, $8, '', now(), now(), $9)";

/// Raises a question on a turn of its own whose evidence names `GITHUB`.
async fn question_naming_the_service(lane: &Lane) -> Uuid7 {
    let action = mint();
    let turn = lane.asking_turn().await;
    sqlx::query(QUESTION_NAMING_THE_SERVICE)
        .bind(mint())
        .bind(&lane.fleet)
        .bind(&lane.workspace)
        .bind(&action)
        .bind(GATE_TYPE_QUESTION)
        .bind(format!("{{\"service\":\"{GITHUB}\"}}"))
        .bind(QUESTION_WINDOW)
        .bind(afd_api_wire::approval::status::PENDING)
        .bind(&turn)
        .execute(&mut *lane.pool.acquire().await.expect("the lane must answer"))
        .await
        .expect("the question is an ordinary gate row");
    action
}

/// Approving, denying or answering a question moves no grant, while the
/// grant's own card stays open.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn answering_a_question_that_names_a_service_moves_no_grant() {
    let lane = Lane::isolated().await;
    let turn = lane.asking_turn().await;
    ask(&lane, GITHUB, GrantKind::Integration, None, &turn).await;
    let before = grant(&lane, GITHUB).await;

    for decision in [Decision::Approved, Decision::Denied, Decision::Answered] {
        let question = question_naming_the_service(&lane).await;
        answer(&lane, &question, decision).await;

        let after = grant(&lane, GITHUB).await;
        assert_eq!(after.status, status::PENDING, "{decision:?}");
        assert_eq!(after, before, "{decision:?}: the grant row is untouched");
    }
}
