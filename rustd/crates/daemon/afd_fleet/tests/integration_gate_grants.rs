//! The approved-grant read against a live Postgres.
//!
//! What is proven is the three-way collapse: absent, `pending` and `revoked`
//! must all be absent from the answer. The failure that would hide is silent
//! in the dangerous direction — a run holding a secret or minting against an
//! integration a person only ever *requested* — and no type catches it,
//! because every one of those rows is a perfectly well-formed grant row.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing datastores, and `make test-integration-rustd` — which runs
//! `--ignored` and nothing else — is the only lane that executes them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use crate::seed;
use crate::support;
use af_core::id::Uuid7;
use afd_api_wire::grant::{grant_type, status};
use afd_db::test_util::GrantSeed;
use afd_gate::policy::approved::Approved;

use self::seed::{Seeded, seeded};
use self::support::Fixtures;

/// Writes one grant row for `fleet`.
async fn grant(fixtures: &Fixtures, fleet: &Uuid7, service: &str, of: &str, state: &str) {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    GrantSeed::new(fleet, service, of, state)
        .insert(&mut *connection)
        .await
        .expect("the grant row must insert");
}

/// `fleet`'s approved grants, as a run's policy build reads them.
async fn approved(fixtures: &Fixtures, fleet: &Uuid7) -> Approved {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    Approved::read_on(&mut connection, fleet)
        .await
        .expect("the grant read must run")
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_only_approved_grants_reach_the_set() {
    // `pending` is a person who has been ASKED and has not answered;
    // `revoked` is a person who answered no, or changed their mind. Admitting
    // either would hand a run an authority nobody currently grants.
    let fixtures = Fixtures::create().await;
    let Seeded { fleet, .. } = seeded::<0>(&fixtures).await;
    grant(
        &fixtures,
        &fleet,
        "github",
        grant_type::INTEGRATION,
        status::APPROVED,
    )
    .await;
    grant(
        &fixtures,
        &fleet,
        "sentry",
        grant_type::SECRET,
        status::APPROVED,
    )
    .await;
    grant(
        &fixtures,
        &fleet,
        "zoho",
        grant_type::SECRET,
        status::PENDING,
    )
    .await;
    grant(
        &fixtures,
        &fleet,
        "slack",
        grant_type::INTEGRATION,
        status::REVOKED,
    )
    .await;

    let granted = approved(&fixtures, &fleet).await;

    assert!(granted.holds("github", grant_type::INTEGRATION));
    assert!(granted.holds("sentry", grant_type::SECRET));
    assert!(
        !granted.holds("sentry", grant_type::INTEGRATION),
        "a grant is of its kind"
    );
    assert!(
        !granted.holds("zoho", grant_type::SECRET),
        "a pending grant is not a grant"
    );
    assert!(
        !granted.holds("slack", grant_type::INTEGRATION),
        "a revoked grant is not a grant"
    );
    assert!(!granted.holds("stripe", grant_type::SECRET));
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_fleet_with_no_grants_holds_nothing_and_is_not_an_error() {
    let fixtures = Fixtures::create().await;
    let Seeded { fleet, .. } = seeded::<0>(&fixtures).await;

    assert_eq!(approved(&fixtures, &fleet).await.names().len(), 0);
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_another_fleets_grant_is_not_this_fleets() {
    // The row carries a `fleet_id` and the statement filters on it. Without
    // that predicate every fleet in a workspace would inherit every other
    // fleet's standing approvals.
    let fixtures = Fixtures::create().await;
    let Seeded { fleet: mine, .. } = seeded::<0>(&fixtures).await;
    let Seeded { fleet: theirs, .. } = seeded::<0>(&fixtures).await;
    grant(
        &fixtures,
        &theirs,
        "github",
        grant_type::INTEGRATION,
        status::APPROVED,
    )
    .await;

    assert!(
        !approved(&fixtures, &mine)
            .await
            .holds("github", grant_type::INTEGRATION)
    );
}
