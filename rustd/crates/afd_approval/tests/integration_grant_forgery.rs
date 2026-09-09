//! A forged gate kind moves no grant.
//!
//! # The escalation this file exists to refuse
//!
//! `RESOLVE_GATE`'s `granted` arm decides whether answering a card also moves a
//! fleet's standing permission to mint a third party's credentials. Two of the
//! three columns it keyed on are reachable by the fleet itself: `gate_kind`
//! travels verbatim from the matched rule in the fleet's own config
//! (`afd_gate::gate::detail::Stated::under`, and the raw schema validated it for
//! length alone), and `evidence` is read out of the event body. So a fleet could
//! declare `gate_kind: "integration_grant"` on any benign tool, emit an event
//! carrying `{"evidence":{"service":"github"}}`, and the ordinary-looking card
//! that raised would — on approval — hand that fleet the credential grant. The
//! operator answering it was told they were approving a tool call.
//!
//! `repository_write`, the daemon's other privileged kind, was already defended
//! twice: `Stated::write_kind` overwrites an authored kind on that path, and
//! `SELECT_APPROVED_WRITE_GATE` demands a recorded binding and this build's
//! ceiling, neither of which a rules-path card carries. `integration_grant` had
//! nothing, and was harmless only because no grant row existed for the arm to
//! move — which is exactly what M194 changed.
//!
//! # Why the event column is the discriminator
//!
//! It is the one thing on the row a fleet cannot reach. `REQUEST_GRANT` writes
//! NULL there by construction (Invariant 5 — a continuation event beside a
//! still-leasable delivery would run the work twice), and every rules-path gate
//! carries a real one, because `afd_gate`'s insert binds `event_id: &str` rather
//! than an `Option`. There is no way to author a card that is both fleet-raised
//! and event-less.
//!
//! These need a live datastore because the claim is a claim about ONE statement:
//! that `granted` declines to move a row. A stub would assert that the resolve
//! calls a statement, which was never in doubt.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_approval::{Decision, IntegrationGrants, KIND_INTEGRATION_GRANT, Origin, Wanted};
use afd_crypto::entropy::Entropy;
use afd_wire::approval::status as gate_status;
use afd_wire::grant::status;
use sqlx::Row as _;

use crate::lane::{Lane, NOW_MS, mint};

/// The service both the honest card and the forged one name.
const SERVICE: &str = "github";

/// Who the fixture records as answering a card.
const REVIEWER: &str = "operator@fixture";

/// An event id, which every rules-path card carries and no grant card does.
const FORGED_EVENT: &str = "01890a5d-ac96-774b-bcce-b302099a8058";

/// The card a fleet-authored rule of this kind would produce.
///
/// `REQUEST_GRANT`'s own column list, so a row that is accepted or refused here
/// is the row the rules path would write and not a thinner fixture the index or
/// the resolve might treat differently. The only deliberate difference is the
/// event id, which is the discriminator under test.
const FORGED_CARD: &str = "\
INSERT INTO core.fleet_approval_gates
  (id, fleet_id, workspace_id, action_id, tool_name, action_name,
   gate_kind, proposed_action, evidence, blast_radius, timeout_at,
   resolved_by, status, detail, created_at, event_id, stated_binding,
   spend_count, spend_ceiling)
VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, $6,
        $7, $8, $9::jsonb, $10, $11,
        '', $12, '', $13, $14, NULL, NULL, NULL)";

/// Raises the honest grant + card through the statement under test.
async fn raise_honest_grant(lane: &Lane) {
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
        .expect("the honest request must land");
}

/// Writes the card a fleet-authored rule would produce: same kind, same service
/// evidence, and the event id such a card always carries.
///
/// Straight into the table on purpose. The point is not whether the rules path
/// can be driven from here, it is whether the RESOLVE would honour such a row —
/// so the row is written in its most favourable form and the resolve is asked.
async fn insert_forged_card(lane: &Lane, action: &str) {
    sqlx::query(FORGED_CARD)
        .bind(mint().as_str())
        .bind(lane.fleet.as_str())
        .bind(lane.workspace.as_str())
        .bind(action)
        .bind("shell")
        .bind("run")
        .bind(KIND_INTEGRATION_GRANT)
        .bind("run a routine build step")
        .bind(format!("{{\"service\":\"{SERVICE}\"}}"))
        .bind("this repository")
        .bind(NOW_MS + 3_600_000)
        .bind(gate_status::PENDING)
        .bind(NOW_MS)
        .bind(FORGED_EVENT)
        .execute(&mut *lane.pool.acquire().await.expect("the lane must answer"))
        .await
        .expect("the forged card is an ordinary gate row");
}

/// Simulates an expired honest card, leaving its grant awaiting approval.
async fn sweep_honest_card(lane: &Lane) {
    sqlx::query(
        "UPDATE core.fleet_approval_gates SET status = $2, active_grant_id = NULL
          WHERE fleet_id = $1::uuid AND gate_kind = $3",
    )
    .bind(lane.fleet.as_str())
    .bind(gate_status::TIMED_OUT)
    .bind(KIND_INTEGRATION_GRANT)
    .execute(&mut *lane.pool.acquire().await.expect("the lane must answer"))
    .await
    .expect("a pending card may be moved to a terminal status");
}

/// The grant's status, or nothing when the fleet holds none.
async fn grant_status(lane: &Lane) -> Option<String> {
    sqlx::query("SELECT status FROM core.integration_grants WHERE fleet_id = $1::uuid")
        .bind(lane.fleet.as_str())
        .fetch_optional(&mut *lane.pool.acquire().await.expect("the lane must answer"))
        .await
        .expect("the grant row is readable")
        .map(|row| row.try_get(0).expect("status"))
}

/// Approving a fleet-authored card does NOT grant the integration.
///
/// The headline. Everything about this card is what the escalation needs — the
/// daemon's kind, the matching service evidence, a pending status, the fleet's
/// own workspace — except that it carries an event id, and that is enough.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn approving_a_card_that_carries_an_event_moves_no_grant() {
    let lane = Lane::isolated().await;
    raise_honest_grant(&lane).await;
    sweep_honest_card(&lane).await;
    let forged = mint().as_str().to_owned();
    insert_forged_card(&lane, &forged).await;

    lane.inbox
        .resolve(
            &forged,
            Decision::Approved,
            REVIEWER,
            "",
            Some(lane.fleet.as_str()),
            Lane::now(),
        )
        .await
        .expect("the resolve itself still runs — the card is answerable");

    assert_eq!(
        grant_status(&lane).await.as_deref(),
        Some(status::PENDING),
        "an event-bearing card must leave the grant exactly as it found it"
    );
}

/// And denying one cannot revoke a grant either.
///
/// The inverse matters as much: the same arm writes `revoked` on a denial, so an
/// unguarded forgery would also let a fleet take away its own standing yes — or,
/// worse, get an operator to do it while answering about something else.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn denying_a_card_that_carries_an_event_revokes_no_grant() {
    let lane = Lane::isolated().await;
    raise_honest_grant(&lane).await;
    sweep_honest_card(&lane).await;
    let forged = mint().as_str().to_owned();
    insert_forged_card(&lane, &forged).await;

    lane.inbox
        .resolve(
            &forged,
            Decision::Denied,
            REVIEWER,
            "",
            Some(lane.fleet.as_str()),
            Lane::now(),
        )
        .await
        .expect("the resolve itself still runs");

    assert_eq!(
        grant_status(&lane).await.as_deref(),
        Some(status::PENDING),
        "a denial on an event-bearing card must not revoke the standing grant"
    );
}

/// A rules-path forgery holds no active grant reference and cannot move the
/// standing grant even while its real card remains actionable.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_forged_approval_cannot_move_a_grant_with_an_open_honest_card() {
    let lane = Lane::isolated().await;
    raise_honest_grant(&lane).await;
    let forged = mint().as_str().to_owned();
    insert_forged_card(&lane, &forged).await;
    lane.inbox
        .resolve(
            &forged,
            Decision::Approved,
            REVIEWER,
            "",
            Some(lane.fleet.as_str()),
            Lane::now(),
        )
        .await
        .expect("resolve runs");
    assert_eq!(grant_status(&lane).await.as_deref(), Some(status::PENDING));
}
