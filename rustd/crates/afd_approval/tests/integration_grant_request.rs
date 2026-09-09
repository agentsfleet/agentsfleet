//! The request verb, and the approve half it finally reaches.
//!
//! # Why these need a real datastore
//!
//! Every claim is a claim about ONE statement. The two writes are atomic
//! because they are one data-modifying CTE; the second card is refused by a
//! `NOT EXISTS` reading the statement's own snapshot; the duplicate grant is
//! refused by `uq_integration_grants_fleet_id_service`. None of that is code a
//! stub could stand in for — a fake would assert that the request calls a
//! statement, which was never in doubt.
//!
//! # And why the approve arm is asserted HERE
//!
//! `RESOLVE_GATE`'s `granted` CTE joins `g.service = r.evidence->>'service'`.
//! It has been correct and unreachable since it was written, because nothing
//! wrote a row for it to move. A test that seeded a grant by hand would prove
//! the join against a fixture's idea of the row; what has to be proven is the
//! join against the row the REQUEST writes, so these two verbs are exercised
//! back to back in one lane.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_approval::{
    Decision, IntegrationGrants, KIND_INTEGRATION_GRANT, Origin, REASON_DECLARED_AT_INSTALL,
    Requested, Resolution, Wanted,
};
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_wire::grant::status;
use sqlx::Row as _;

use crate::lane::{Lane, NOW_MS};

/// The service every request in this suite asks about.
const SERVICE: &str = "github";

/// The name the fixture fleet declared that service's credential under.
///
/// Deliberately NOT the service's own spelling: the card names the fleet's
/// declaration and the grant names the connector, and a fixture where the two
/// matched could not tell a swapped pair apart.
const CREDENTIAL: &str = "gh";

/// How many consecutive polls stand in for a parked delivery redelivering.
///
/// Ten, which at `NO_WORK_RETRY_AFTER_MS` is ten seconds of a parked event.
const POLLS: usize = 10;

/// Who the fixture records as answering a card.
const REVIEWER: &str = "operator@fixture";

/// The store under test, over the lane's pool.
fn grants(lane: &Lane) -> IntegrationGrants {
    IntegrationGrants::new(lane.pool.clone(), Entropy::new())
}

/// What the fixture asks for.
const fn wanted() -> Wanted<'static> {
    Wanted {
        service: SERVICE,
        credential: CREDENTIAL,
        origin: Origin::Install,
    }
}

/// Requests one grant on `lane`'s fleet.
async fn request(lane: &Lane) -> Requested {
    grants(lane)
        .request(&lane.workspace, &lane.fleet, wanted(), Lane::now())
        .await
        .expect("the request must run")
}

/// Every grant row the lane's fleet holds, as `(service, status, reason)`.
async fn grant_rows(lane: &Lane) -> Vec<(String, String, String)> {
    sqlx::query(
        "SELECT service, status, requested_reason
           FROM core.integration_grants WHERE fleet_id = $1::uuid",
    )
    .bind(lane.fleet.as_str())
    .fetch_all(&mut *lane.pool.acquire().await.expect("the lane must answer"))
    .await
    .expect("the grant rows are readable")
    .iter()
    .map(|row| {
        (
            row.try_get(0).expect("service"),
            row.try_get(1).expect("status"),
            row.try_get(2).expect("reason"),
        )
    })
    .collect()
}

/// Every gate this suite's kind raised on the lane's fleet, as
/// `(action_id, evidence service, event_id)`.
async fn cards(lane: &Lane) -> Vec<(String, Option<String>, Option<String>)> {
    sqlx::query(
        "SELECT action_id, evidence->>'service', event_id
           FROM core.fleet_approval_gates
          WHERE fleet_id = $1::uuid AND gate_kind = $2",
    )
    .bind(lane.fleet.as_str())
    .bind(KIND_INTEGRATION_GRANT)
    .fetch_all(&mut *lane.pool.acquire().await.expect("the lane must answer"))
    .await
    .expect("the gate rows are readable")
    .iter()
    .map(|row| {
        (
            row.try_get(0).expect("action_id"),
            row.try_get(1).expect("evidence service"),
            row.try_get(2).expect("event_id"),
        )
    })
    .collect()
}

/// The action id of the one card this fleet holds.
///
/// A named helper rather than `cards(..).remove(0)` at four call sites: the
/// suite asserts elsewhere that exactly one card exists, and a panic here says
/// which precondition failed rather than reporting an index out of bounds.
async fn first_card_action(lane: &Lane) -> String {
    cards(lane)
        .await
        .into_iter()
        .next()
        .expect("a request raised a card to answer")
        .0
}

/// A declared mintable credential leaves the request with a row and a card.
///
/// The headline. Before this verb existed the row could not be created at all,
/// so a fleet declaring `github` parked every delivery on a decision nobody
/// could be asked for.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_request_writes_the_grant_and_the_card_together() {
    let lane = Lane::isolated().await;

    assert_eq!(request(&lane).await, Requested::Raised);

    assert_eq!(
        grant_rows(&lane).await,
        vec![(
            SERVICE.to_owned(),
            status::PENDING.to_owned(),
            REASON_DECLARED_AT_INSTALL.to_owned()
        )]
    );
    let raised = cards(&lane).await;
    assert_eq!(raised.len(), 1);
    let only = raised.first().expect("the request raised exactly one card");
    // The key `RESOLVE_GATE` joins on. A card without it resolves cleanly and
    // moves no grant — a failure with no error, which is the class this
    // milestone exists to end.
    assert_eq!(only.1.as_deref(), Some(SERVICE));
    // And NO event. A grant card carrying one would land a continuation event
    // beside the still-leasable delivery, and the fleet would run the work
    // twice.
    assert_eq!(only.2, None);
}

/// A repeated install writes one row, and the table is what says so.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_repeated_request_does_not_duplicate_the_grant() {
    let lane = Lane::isolated().await;

    assert_eq!(request(&lane).await, Requested::Raised);
    assert_eq!(request(&lane).await, Requested::Pending);

    assert_eq!(grant_rows(&lane).await.len(), 1);
}

/// A park redelivering every second raises one card, not one per second.
///
/// Ten consecutive requests, which at the one-second redelivery cadence is ten
/// seconds of a parked delivery. The guard is a pre-insert `NOT EXISTS` and not
/// a rate limit, which is the difference between "one question stands" and "one
/// question a minute".
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_redelivering_park_raises_one_card_not_one_per_second() {
    let lane = Lane::isolated().await;

    let mut outcomes = Vec::with_capacity(POLLS);
    for _poll in 0..POLLS {
        outcomes.push(request(&lane).await);
    }

    let (first, rest) = outcomes.split_first().expect("ten polls answered");
    assert_eq!(*first, Requested::Raised);
    assert!(
        rest.iter().all(|seen| *seen == Requested::Pending),
        "{outcomes:?}"
    );
    assert_eq!(cards(&lane).await.len(), 1);
    assert_eq!(grant_rows(&lane).await.len(), 1);
}

/// Approving the card grants the integration, in the statement that resolved it.
///
/// The approve half, reached for the first time by a row this crate wrote.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn approving_the_card_grants_the_integration() {
    let lane = Lane::isolated().await;
    request(&lane).await;
    let action = first_card_action(&lane).await;

    let resolved = lane
        .inbox
        .resolve(
            &action,
            Decision::Approved,
            REVIEWER,
            "",
            Some(lane.fleet.as_str()),
            Lane::now(),
        )
        .await
        .expect("the resolve must run");

    assert!(matches!(resolved, Resolution::Resolved(_)), "{resolved:?}");
    let granted = grant_rows(&lane).await;
    let moved = granted
        .first()
        .expect("the fleet holds its requested grant");
    assert_eq!(moved.1, status::APPROVED);
    assert_eq!(approved_at(&lane).await, Some(NOW_MS));
}

/// Denying the card revokes the grant rather than leaving it pending.
///
/// The half that ends a parked event: the delivery reads `revoked` on its next
/// poll and writes a terminal row instead of parking again. Proven on the lease
/// path in `afd_fleet`; proven HERE as the column transition it depends on.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn denying_the_card_revokes_the_grant() {
    let lane = Lane::isolated().await;
    request(&lane).await;
    let action = first_card_action(&lane).await;

    lane.inbox
        .resolve(
            &action,
            Decision::Denied,
            REVIEWER,
            "",
            Some(lane.fleet.as_str()),
            Lane::now(),
        )
        .await
        .expect("the resolve must run");

    let denied = grant_rows(&lane).await;
    let taken_back = denied.first().expect("the fleet holds its requested grant");
    assert_eq!(taken_back.1, status::REVOKED);
    // A denied grant is never re-asked. Talking over a person's no every second
    // is the loop with a card on it rather than the loop without one.
    assert_eq!(request(&lane).await, Requested::Denied);
    assert_eq!(cards(&lane).await.len(), 1);
}

/// A standing yes is not asked about again.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_approved_grant_raises_no_second_card() {
    let lane = Lane::isolated().await;
    request(&lane).await;
    let action = first_card_action(&lane).await;
    lane.inbox
        .resolve(
            &action,
            Decision::Approved,
            REVIEWER,
            "",
            Some(lane.fleet.as_str()),
            Lane::now(),
        )
        .await
        .expect("the resolve must run");

    assert_eq!(request(&lane).await, Requested::Approved);

    assert_eq!(cards(&lane).await.len(), 1);
}

/// When the grant was approved, as the resolve stamped it.
async fn approved_at(lane: &Lane) -> Option<i64> {
    sqlx::query("SELECT approved_at FROM core.integration_grants WHERE fleet_id = $1::uuid")
        .bind(lane.fleet.as_str())
        .fetch_one(&mut *lane.pool.acquire().await.expect("the lane must answer"))
        .await
        .expect("the grant row is readable")
        .try_get(0)
        .expect("approved_at is readable")
}

/// One fleet's request is not another's.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_request_names_the_fleet_that_asked() {
    let mine = Lane::isolated().await;
    let theirs = Lane::isolated().await;

    request(&mine).await;

    assert_eq!(grant_rows(&theirs).await, Vec::new());
    assert_eq!(cards(&theirs).await, Vec::new());
    assert_ne!(mine.fleet.as_str(), Uuid7::as_str(&theirs.fleet));
}
