//! What authorises a write mint now, and what no longer gets a say.
//!
//! M202 deleted the second gate. A fleet's WRITE binding used to park every
//! first-encounter event and the mint then re-read the answered card, so a
//! continuation — which carries a fresh event identifier — was seen as a first
//! encounter and parked again: three cards and two approvals for one steer.
//! `admit_mint` had always read the standing grant first; the card above it was
//! asking a person a question the grant had already answered.
//!
//! So the card's STATE is no longer an input to the mint, and that is what this
//! file proves from the mint's side: absent, drifted and spent cards all reach
//! the same place, because none of them is consulted. The one thing that still
//! decides is the standing grant, which the last case removes to show it.
//!
//! Reaching `CRED_INTEGRATION_NOT_CONNECTED` is the PASS signal here. The
//! fixture deliberately stores no handle, so a mint that gets all the way to the
//! vault can only fail there — which is the proof it was never turned back
//! earlier.
//!
//! Every case declares a WRITE binding on the `github` connector, the only pair
//! the deleted gate ever applied to.

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
async fn test_the_standing_grant_alone_carries_a_write_mint_to_the_vault() {
    // No card was ever raised. Before M202 this refused as unapproved; now the
    // approved grant is the whole answer and the mint walks to the vault.
    support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let owner = write_bound(&fixtures).await;

    assert_eq!(
        refusal_code(&fixtures, &owner).await,
        error_code::CRED_INTEGRATION_NOT_CONNECTED,
        "a granted write mint was turned back before the vault"
    );

    fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires a live Postgres; run through `make test-integration-rustd`"]
async fn test_a_drifted_card_does_not_block_a_mint_the_grant_authorises() {
    // The card was answered for `acme/ledger`; the fleet declares
    // `acme/payments`. That mismatch used to refuse as drift. Nothing reads it
    // now, so the mint is unaffected — the assertion is that the card's reach
    // makes no difference at all.
    support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let owner = write_bound(&fixtures).await;
    let _gate = seed_write_gate(&fixtures, &owner.fleet, DRIFTED_BINDING, 0).await;

    assert_eq!(
        refusal_code(&fixtures, &owner).await,
        error_code::CRED_INTEGRATION_NOT_CONNECTED,
        "a stale card still reached the mint"
    );

    fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires a live Postgres; run through `make test-integration-rustd`"]
async fn test_a_spent_allowance_does_not_block_a_mint_the_grant_authorises() {
    // Same shape, the other half of the deleted gate: the allowance the card was
    // raised with is gone. It used to refuse as exhausted; the ceiling is no
    // longer consulted at mint time either.
    support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let owner = write_bound(&fixtures).await;
    let _gate = seed_write_gate(&fixtures, &owner.fleet, STATED_BINDING, WRITE_SPEND_CEILING).await;

    assert_eq!(
        refusal_code(&fixtures, &owner).await,
        error_code::CRED_INTEGRATION_NOT_CONNECTED,
        "a spent card still reached the mint"
    );

    fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires a live Postgres; run through `make test-integration-rustd`"]
async fn test_a_write_mint_without_a_standing_grant_is_refused() {
    // The negative that keeps the three above honest. Remove the grant and the
    // mint refuses — so those cases pass because the grant authorised them, not
    // because nothing is checked. An approved card is seeded here to make the
    // point sharply: a card cannot stand in for the grant.
    support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let owner = bound(&fixtures, WRITE_BOUND_CONFIG).await;
    let _gate = seed_write_gate(&fixtures, &owner.fleet, STATED_BINDING, 0).await;

    assert_eq!(
        refusal_code(&fixtures, &owner).await,
        error_code::GRANT_NOT_FOUND,
        "a write mint with no standing grant was allowed through"
    );

    fixtures.cleanup().await;
}
