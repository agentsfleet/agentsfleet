//! The install verb: a grant that lands answered, and no card at all.
//!
//! # Why these need a real datastore
//!
//! Every claim here is a property of the row, not of the code that writes it.
//! `uq_integration_grants_fleet_id_service` is what makes a re-install move
//! nothing, and `ON CONFLICT DO NOTHING` is what makes a revoked grant outlive
//! the next install. Neither is visible without the real schema — a fake store
//! would assert the fixture's idea of a unique constraint.
//!
//! # The card count is the headline
//!
//! A reviewer fleet used to raise one approval card per model turn and post
//! zero reviews. The install half of that is asserted here: a fleet that
//! declares a mintable credential holds a standing yes and an empty inbox, so
//! nothing downstream has a card to wait on.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_approval::{
    Decision, IntegrationGrants, KIND_INTEGRATION_GRANT, Origin, REASON_DECLARED_AT_INSTALL,
    Requested, Wanted,
};
use afd_crypto::entropy::Entropy;
use afd_wire::grant::status;
use sqlx::Row as _;

use crate::lane::{Lane, NOW_MS};

/// The service every install in this suite declares.
const SERVICE: &str = "github";

/// The name the fixture fleet declared that service's credential under.
///
/// Deliberately NOT the service's own spelling, for the reason the request
/// suite gives: the grant names the connector and the card names the fleet's
/// declaration, so a fixture where the two matched could not tell a swapped
/// pair apart.
const CREDENTIAL: &str = "gh";

/// Who the fixture records as answering a card.
const REVIEWER: &str = "operator@fixture";

/// How many consecutive installs stand in for an operator reinstalling.
const REINSTALLS: usize = 3;

/// The store under test, over the lane's pool.
fn grants(lane: &Lane) -> IntegrationGrants {
    IntegrationGrants::new(lane.pool.clone(), Entropy::new())
}

/// What the fixture's bundle declared.
const fn declared() -> Wanted<'static> {
    Wanted {
        service: SERVICE,
        credential: CREDENTIAL,
        origin: Origin::Install,
    }
}

/// Installs the fixture's declared credential on `lane`'s fleet.
async fn install(lane: &Lane) -> Requested {
    grants(lane)
        .grant_at_install(&lane.fleet, declared(), Lane::now())
        .await
        .expect("the install must run")
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

/// When the grant was approved, as the install stamped it.
async fn approved_at(lane: &Lane) -> Option<i64> {
    sqlx::query("SELECT approved_at FROM core.integration_grants WHERE fleet_id = $1::uuid")
        .bind(lane.fleet.as_str())
        .fetch_one(&mut *lane.pool.acquire().await.expect("the lane must answer"))
        .await
        .expect("the grant row is readable")
        .try_get(0)
        .expect("approved_at is readable")
}

/// Every approval card of ANY kind the lane's fleet holds.
///
/// Deliberately unfiltered by `gate_kind`: the claim is that an install raises
/// nothing, and a count filtered to one kind would pass while the install
/// raised a card of another.
async fn card_count(lane: &Lane) -> i64 {
    sqlx::query("SELECT COUNT(*) FROM core.fleet_approval_gates WHERE fleet_id = $1::uuid")
        .bind(lane.fleet.as_str())
        .fetch_one(&mut *lane.pool.acquire().await.expect("the lane must answer"))
        .await
        .expect("the gate rows are readable")
        .try_get(0)
        .expect("the count is readable")
}

/// The action id of the one card the park path raised.
async fn parked_card_action(lane: &Lane) -> String {
    sqlx::query(
        "SELECT action_id FROM core.fleet_approval_gates
          WHERE fleet_id = $1::uuid AND gate_kind = $2",
    )
    .bind(lane.fleet.as_str())
    .bind(KIND_INTEGRATION_GRANT)
    .fetch_one(&mut *lane.pool.acquire().await.expect("the lane must answer"))
    .await
    .expect("a card was raised to answer")
    .try_get(0)
    .expect("action_id is readable")
}

/// Dimension 1.1 — an install lands one approved grant, stamped.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_m202_001_install_lands_an_approved_grant() {
    let lane = Lane::isolated().await;

    assert_eq!(install(&lane).await, Requested::Approved);

    assert_eq!(
        grant_rows(&lane).await,
        vec![(
            SERVICE.to_owned(),
            status::APPROVED.to_owned(),
            REASON_DECLARED_AT_INSTALL.to_owned()
        )]
    );
    // Non-null, and the install's own instant: nobody was asked, so the moment
    // the row was written IS the moment it was answered. A null here would
    // read on the wire as a grant still waiting for someone.
    assert_eq!(approved_at(&lane).await, Some(NOW_MS));
}

/// Dimension 1.2 — an install raises no card, of any kind.
///
/// The one this milestone is named for. Before it, a fleet declaring `github`
/// got a card at install and then one per model turn after it.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_m202_001_install_raises_no_card() {
    let lane = Lane::isolated().await;

    install(&lane).await;

    assert_eq!(card_count(&lane).await, 0);
}

/// Dimension 1.3 — a re-install moves no second row.
///
/// Three installs rather than two: the unique constraint would catch a second
/// row, and what this also pins is that repeated installs report the SAME
/// standing answer rather than drifting after the first.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_m202_001_reinstall_keeps_one_grant() {
    let lane = Lane::isolated().await;

    let mut outcomes = Vec::with_capacity(REINSTALLS);
    for _again in 0..REINSTALLS {
        outcomes.push(install(&lane).await);
    }

    assert!(
        outcomes.iter().all(|seen| *seen == Requested::Approved),
        "{outcomes:?}"
    );
    assert_eq!(grant_rows(&lane).await.len(), 1);
    assert_eq!(card_count(&lane).await, 0);
}

/// A revoked grant outlives the next install.
///
/// Not a Dimension of its own, and the most important test in this file. Your
/// stop button is `grant delete`; if reinstalling the fleet undid it, the stop
/// button would have a timer on it. The park path raises the card, a person
/// denies it, and the install that follows must find the no still standing.
///
/// This is also what pins `GRANT_AT_INSTALL`'s conflict arm as a NO-OP update.
/// The statement writes `status = core.integration_grants.status` so a conflict
/// still returns its row; an author who changed that to `EXCLUDED.status` to
/// "fix" a re-install would un-revoke every grant, and this test is what fails.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_m202_001_reinstall_never_undoes_a_revoke() {
    let lane = Lane::isolated().await;
    let parked = Wanted {
        origin: Origin::Park,
        ..declared()
    };
    grants(&lane)
        .request(&lane.workspace, &lane.fleet, parked, Lane::now())
        .await
        .expect("the park backstop must run");
    let action = parked_card_action(&lane).await;
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

    assert_eq!(install(&lane).await, Requested::Denied);

    let standing = grant_rows(&lane).await;
    let refused = standing.first().expect("the fleet holds its grant");
    assert_eq!(refused.1, status::REVOKED);
    assert_eq!(standing.len(), 1);
}

/// One fleet's install is not another's.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_m202_001_install_names_the_fleet_that_declared_it() {
    let mine = Lane::isolated().await;
    let theirs = Lane::isolated().await;

    install(&mine).await;

    assert_eq!(grant_rows(&theirs).await, Vec::new());
    assert_eq!(card_count(&theirs).await, 0);
}
