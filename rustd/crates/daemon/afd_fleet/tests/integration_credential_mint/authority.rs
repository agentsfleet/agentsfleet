//! A runner's credential authority is the fleet's approved grants as they
//! stand at each mint, and it does not survive cancellation.

use super::*;
use crate::requests;

async fn mint_static(
    fixtures: &Fixtures,
    owner: &Bound,
) -> afd_fleet::error::Result<afd_credential::credential::Minted> {
    fixtures
        .plane()
        .mint(
            &owner.runner,
            &requests::mint(owner.run.as_str(), CONNECTOR_STATIC),
            fixtures.enrolled_at(),
        )
        .await
}

async fn seed_static(fixtures: &Fixtures, owner: &Bound) {
    seed_handle(
        fixtures,
        &owner.workspace,
        CONNECTOR_STATIC,
        &format!(r#"{{"integration":"static","token":"{OWNER_TOKEN}"}}"#),
    )
    .await;
}

// Value: protects=a run mints only what a person approved for its fleet, read
// at the mint, so a revoke reaches a run already in flight; fails_when=the mint
// reads the grants at assignment, or treats a revoked grant as standing;
// seam=the production mint verb over live Postgres.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn runner_mint_follows_the_fleets_approved_grants() {
    let fixtures = Fixtures::create().await;
    let owner = bound(&fixtures).await;
    seed_static(&fixtures, &owner).await;
    assert_eq!(
        mint_static(&fixtures, &owner)
            .await
            .expect_err("nothing approved, nothing minted")
            .code(),
        error_code::GRANT_NOT_FOUND
    );
    seed_grant(&fixtures, &owner.fleet, CONNECTOR_STATIC, STATUS_APPROVED).await;
    assert_eq!(
        mint_static(&fixtures, &owner)
            .await
            .expect("an approved grant mints")
            .token
            .as_str(),
        OWNER_TOKEN
    );
    seed_grant(&fixtures, &owner.fleet, CONNECTOR_STATIC, STATUS_REVOKED).await;
    assert_eq!(
        mint_static(&fixtures, &owner)
            .await
            .expect_err("a revoked grant mints nothing")
            .code(),
        error_code::GRANT_NOT_FOUND
    );
    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn cancellation_committed_before_a_new_mint_withholds_credentials() {
    let fixtures = Fixtures::create().await;
    let owner = bound(&fixtures).await;
    seed_static(&fixtures, &owner).await;
    seed_grant(&fixtures, &owner.fleet, CONNECTOR_STATIC, STATUS_APPROVED).await;
    mint_static(&fixtures, &owner)
        .await
        .expect("active control");
    sqlx::query("UPDATE thread.runs SET cancel_requested_at=clock_timestamp() WHERE id=$1")
        .bind(&owner.run)
        .execute(&mut *fixtures.database.acquire().await.expect("connection"))
        .await
        .expect("durable cancellation");
    assert_eq!(
        mint_static(&fixtures, &owner)
            .await
            .expect_err("cancellation removes authority")
            .code(),
        error_code::RUN_NOT_FOUND
    );
    fixtures.cleanup().await;
}
