//! The lease claim and the platform read name a key by the same witnesses, so
//! a key one lease read is found again by the next claim.
//!
//! The two are separate statements with the same digest expression spelled in
//! each. This runs the real claim beside the real read over one secret and
//! compares what they return, once with no endpoint pinned and once with one,
//! so an expression that dropped a field from either side would show here.
//! A private database: the second case writes the secret's row.

use std::sync::Arc;

use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use af_crypto::secret::Kek;
use afd_credential::provider::Providers;

use super::seed::{seed_provider_resolution, workspace_of};
use super::{claim, ready};
use crate::report_seed::FIXTURE_KEK_HEX;
use crate::seed::PROVIDER;
use crate::support::Fixtures;

/// The base URL the seeded model's saved settings carry: the one endpoint a
/// credential of this provider may be pinned to.
const SAVED_BASE_URL: &str = "https://api.anthropic.com/v1";

/// Pins the platform credential to `url`, as a metadata-only write would.
async fn pin_endpoint(fixtures: &Fixtures, fleet: &Uuid7, url: &str) {
    let workspace = workspace_of(fixtures, fleet).await;
    sqlx::query(
        "UPDATE vault.secrets SET meta_base_url = $3 WHERE workspace_id = $1 AND key_name = $2",
    )
    .bind(&workspace)
    .bind(PROVIDER)
    .bind(url)
    .execute(
        &mut *fixtures
            .database
            .acquire()
            .await
            .expect("a pooled connection"),
    )
    .await
    .expect("the credential is pinned");
}

/// Claims a fleet over the seeded default and reads the default as a lease
/// would, then asks that the two agree.
async fn the_claim_and_the_read_name_one_key(endpoint: Option<&str>) {
    let fixtures = Fixtures::create_isolated().await;
    let ready = ready(&fixtures).await;
    let _default = seed_provider_resolution(&fixtures, &ready.fleet).await;
    if let Some(url) = endpoint {
        pin_endpoint(&fixtures, &ready.fleet, url).await;
    }
    let claimed = claim(&fixtures, &ready).await;
    let named = claimed
        .opened
        .platform_key
        .clone()
        .expect("the claim read the platform default's secret");

    let providers = Providers::new(
        fixtures.database.clone(),
        Arc::new(Kek::from_hex(FIXTURE_KEK_HEX).expect("the lane key is well formed")),
        Entropy::new(),
    );
    let read = providers
        .observe_platform(&ready.tenant)
        .await
        .expect("the platform read answers")
        .expect("the seeded default routes");

    assert_eq!(
        named, read.key,
        "the claim's catalogue version, secret id and digest are the read's (endpoint {endpoint:?})"
    );
    drop(claimed);
    fixtures.cleanup().await;
}

// Value: protects=a key one lease read is found by the next claim, so the cache
// hits; fails_when=either statement's digest expression drops a field the other
// keeps; why_new=the cache trusts two statements to agree; seam=none;
// no_claim=a key replaced between the claim and the read.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn the_claims_witnesses_are_the_platform_reads() {
    the_claim_and_the_read_name_one_key(None).await;
}

// Value: protects=the same agreement with an endpoint beside the ciphertext;
// fails_when=one side leaves the endpoint out of its digest; why_new=the
// endpoint is part of what a cached resolution answers for; seam=none;
// no_claim=an endpoint the route would refuse.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn the_claims_witnesses_are_the_platform_reads_with_a_pinned_endpoint() {
    the_claim_and_the_read_name_one_key(Some(SAVED_BASE_URL)).await;
}
