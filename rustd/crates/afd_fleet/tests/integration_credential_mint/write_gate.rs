//! The write gate as `Plane::mint` reads it: four verdicts, four refusal codes.
//!
//! The gate's own verdicts are proven next door in `cases.rs`, against the
//! reservation call directly. What is proven here is the MAPPING — that each
//! verdict leaves the mint as its own registry code, so a runner blocked on an
//! answer can tell "nobody approved this" from "the approval no longer matches
//! your reach" from "the allowance is spent". Read through the gate alone,
//! those three are one `WriteApproval` enum and indistinguishable to a child.
//!
//! Every case here declares a WRITE binding on the `github` connector, because
//! that pair is the only one the gate applies to: a read binding, a missing
//! binding, or any other connector returns before the reservation is attempted.
//!
//! No vendor is dialled. The three refusals end at the gate, and the approval
//! case deliberately stores no handle, so it ends at the vault with
//! "not connected" — which is itself the proof that the gate let it through.

use super::*;
use crate::requests;
use afd_core::error_code::ErrorCode;

/// A stored document declaring a write reach over [`super::STATED_BINDING`]'s
/// repository, which is what puts a binding on the lease's mint scope.
const WRITE_BOUND_CONFIG: &str = r#"{"name":"probe","x-agentsfleet":{"triggers":[{"type":"api"}],"tools":[],"budget":{"daily_dollars":1.0},"repositories":["acme/payments"],"repository_access":"write","repository_base":"main"}}"#;

/// A reach the fixture fleet does not declare, for the drift case.
const DRIFTED_BINDING: &str = r#"{"repositories":["acme/ledger"],"access":"write","base":"main"}"#;

/// A write-bound fleet whose `github` grant is approved, ready to mint.
async fn write_bound(fixtures: &Fixtures) -> Bound {
    let owner = bound(fixtures, WRITE_BOUND_CONFIG).await;
    seed_grant(fixtures, &owner.fleet, CONNECTOR_GITHUB, "approved").await;
    owner
}

/// The code a write mint refuses `owner` with, through the whole plane.
async fn refusal_code(fixtures: &Fixtures, owner: &Bound) -> ErrorCode {
    fixtures
        .plane()
        .mint(
            &owner.runner,
            &requests::mint(&owner.lease, CONNECTOR_GITHUB),
            UnixMillis::from_millis(NOW_MS),
        )
        .await
        .expect_err("a write mint without a usable approval must refuse")
        .code()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires a live Postgres; run through `make test-integration-rustd`"]
async fn test_a_write_mint_with_no_gate_at_all_is_unapproved() {
    // The first time a child reaches for a repository write: the fleet declares
    // the reach, the integration is granted, and no human has been asked yet.
    // The runner must be told to raise a card, not that its grant is missing.
    support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let owner = write_bound(&fixtures).await;

    assert_eq!(
        refusal_code(&fixtures, &owner).await,
        error_code::REPAIR_WRITE_UNAPPROVED,
        "an unanswered write gate must not read as a missing grant"
    );

    fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires a live Postgres; run through `make test-integration-rustd`"]
async fn test_a_write_mint_against_a_drifted_approval_is_refused_as_drift() {
    // The card was answered for `acme/ledger`; the fleet now declares
    // `acme/payments`. A human said yes to a reach that is no longer the one
    // being asked for, so the approval cannot be spent on it.
    support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let owner = write_bound(&fixtures).await;
    let _gate = seed_write_gate(&fixtures, &owner.fleet, DRIFTED_BINDING, 0).await;

    assert_eq!(
        refusal_code(&fixtures, &owner).await,
        error_code::REPAIR_BINDING_DRIFT,
        "a stale approval was spent on a reach nobody approved"
    );

    fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires a live Postgres; run through `make test-integration-rustd`"]
async fn test_a_write_mint_against_a_spent_allowance_is_refused_as_exhausted() {
    // The reach matches and a human approved it, and the allowance it was
    // raised with is gone. Distinct from drift because the remedy is
    // different: raise the ceiling, not re-approve the reach.
    support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let owner = write_bound(&fixtures).await;
    let _gate = seed_write_gate(&fixtures, &owner.fleet, STATED_BINDING, WRITE_SPEND_CEILING).await;

    assert_eq!(
        refusal_code(&fixtures, &owner).await,
        error_code::REPAIR_SPEND_EXHAUSTED,
        "a spent allowance was treated as spendable"
    );

    fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires a live Postgres; run through `make test-integration-rustd`"]
async fn test_an_approved_write_gate_carries_the_mint_through_to_the_vault() {
    // The positive, and the only one that proves the gate is not simply
    // refusing everything: matching reach, unspent allowance, and NO stored
    // handle. Reaching "not connected" means the reservation returned
    // `Approved` and the mint walked past it to open the vault.
    support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let owner = write_bound(&fixtures).await;
    let _gate = seed_write_gate(&fixtures, &owner.fleet, STATED_BINDING, 0).await;

    assert_eq!(
        refusal_code(&fixtures, &owner).await,
        error_code::CRED_INTEGRATION_NOT_CONNECTED,
        "an approved write gate did not carry the mint past the reservation"
    );

    fixtures.cleanup().await;
}
