//! The integration-grant read against a live Postgres.
//!
//! What is proven is the three-way collapse: absent, `pending` and `revoked`
//! must all be absent from the answer. The failure that would hide is silent
//! in the dangerous direction — a fleet minting against an integration a human
//! only ever *requested* — and no type catches it, because every one of those
//! rows is a perfectly well-formed grant row.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing datastores, and `make test-integration-rustd` — which runs
//! `--ignored` and nothing else — is the only lane that executes them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "support/fleet_fixtures.rs"]
mod support;

#[path = "support/fleet_queue.rs"]
mod queue;

#[path = "support/fleet_lease_reads.rs"]
mod lease_reads;

#[path = "support/fleet_lease_seed.rs"]
mod seed;

#[path = "support/fleet_requests.rs"]
mod requests;

use afd_core::id::Uuid7;
use afd_fleet::policy::grants::Grants;

use self::seed::{Seeded, seeded};
use self::support::Fixtures;

/// Grant identifiers, distinct and version-7 so the table's CHECK admits them.
const GRANT_IDS: [&str; 3] = [
    "0197a4ba-8d3a-7f13-8abc-1000000000a1",
    "0197a4ba-8d3a-7f13-8abc-1000000000a2",
    "0197a4ba-8d3a-7f13-8abc-1000000000a3",
];

/// Writes one grant row for `fleet`.
async fn grant(fixtures: &Fixtures, id: &str, fleet: &str, service: &str, status: &str) {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO core.integration_grants
           (id, fleet_id, service, status, requested_reason, created_at)
         VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6)",
    )
    .bind(id)
    .bind(fleet)
    .bind(service)
    .bind(status)
    .bind("a fixture asked")
    .bind(requests::ENROLLED_AT)
    .execute(&mut *connection)
    .await
    .expect("the grant row must insert");
}

/// The fleet id, parsed the way the caller holds it.
fn id(fleet: &str) -> Uuid7 {
    Uuid7::parse(fleet).expect("the seeded fleet id is a uuid")
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_only_approved_grants_reach_the_set() {
    // The whole point. `pending` is a human who has been ASKED and has not
    // answered; `revoked` is a human who answered and then changed their mind.
    // Admitting either would mint a third-party token under an authority
    // nobody currently grants.
    let fixtures = Fixtures::create_with_queue().await;
    let Seeded { fleet, .. } = seeded::<0>(&fixtures).await;
    grant(&fixtures, GRANT_IDS[0], &fleet, "github", "approved").await;
    grant(&fixtures, GRANT_IDS[1], &fleet, "zoho", "pending").await;
    grant(&fixtures, GRANT_IDS[2], &fleet, "slack", "revoked").await;

    let granted = fixtures
        .gates()
        .approved_integrations(&id(&fleet))
        .await
        .expect("the grant read must run");

    assert!(granted.holds("github"));
    assert!(!granted.holds("zoho"), "a pending grant is not a grant");
    assert!(!granted.holds("slack"), "a revoked grant is not a grant");
    // Absent joins the other two, which is the reading that makes a fleet
    // declaring an integration nobody ever considered park rather than run.
    assert!(!granted.holds("stripe"));
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_fleet_with_no_grants_holds_nothing_and_is_not_an_error() {
    let fixtures = Fixtures::create_with_queue().await;
    let Seeded { fleet, .. } = seeded::<0>(&fixtures).await;

    let granted = fixtures
        .gates()
        .approved_integrations(&id(&fleet))
        .await
        .expect("no rows is not a datastore failure");

    assert_eq!(granted, Grants::none());
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_another_fleets_grant_is_not_this_fleets() {
    // The row carries a `fleet_id` and the statement filters on it. Without
    // that predicate every fleet in a workspace would inherit every other
    // fleet's standing approvals.
    let fixtures = Fixtures::create_with_queue().await;
    let Seeded { fleet: mine, .. } = seeded::<0>(&fixtures).await;
    let Seeded { fleet: theirs, .. } = seeded::<0>(&fixtures).await;
    grant(&fixtures, GRANT_IDS[0], &theirs, "github", "approved").await;

    let granted = fixtures
        .gates()
        .approved_integrations(&id(&mine))
        .await
        .expect("the grant read must run");

    assert!(!granted.holds("github"));
}
