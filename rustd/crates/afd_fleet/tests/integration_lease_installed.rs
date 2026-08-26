//! §2's claim read against a live Postgres: what a fleet IS at claim time.
//!
//! The statement joins two tables and answers four different ways depending on
//! one column and one absent row, and none of that is observable without a
//! database. What is proven here is the four answers — resolved, stopped,
//! gone, and unreadable — because each one sends the lease path somewhere
//! different, and three of them are silent.
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
use afd_fleet::lease::FRESH_CONTEXT;

use self::seed::{Seeded, seeded};
use self::support::Fixtures;

/// A stored config the parser accepts.
const STORED_CONFIG: &str = r#"{"name":"probe","x-agentsfleet":{"triggers":[{"type":"api"}],
   "tools":[],"budget":{"daily_dollars":1.0}}}"#;

/// A source document whose prose is unmistakable in an assertion.
const SOURCE_MARKDOWN: &str = "---\nname: probe\n---\nAnswer only in haiku.\n";

/// Rewrites the seeded fleet's installed columns.
async fn install(fixtures: &Fixtures, fleet: &str, config: &str, status: &str) {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "UPDATE core.fleets
            SET config_json = $2::jsonb, source_markdown = $3, status = $4
          WHERE id = $1::uuid",
    )
    .bind(fleet)
    .bind(config)
    .bind(SOURCE_MARKDOWN)
    .bind(status)
    .execute(&mut *connection)
    .await
    .expect("the fleet row must update");
}

/// The fleet id, parsed the way the caller holds it.
fn id(fleet: &str) -> Uuid7 {
    Uuid7::parse(fleet).expect("the seeded fleet id is a uuid")
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_an_active_fleet_resolves_its_installed_shape() {
    let fixtures = Fixtures::create_with_queue().await;
    let Seeded { fleet, .. } = seeded::<0>(&fixtures).await;
    install(&fixtures, &fleet, STORED_CONFIG, "active").await;

    let installed = fixtures
        .leases()
        .installed(&id(&fleet))
        .await
        .expect("the claim read must run")
        .expect("an active fleet is runnable");

    assert_eq!(installed.config.name().as_str(), "probe");
    // The prose, and only the prose — the frontmatter must not reach the run.
    assert_eq!(installed.instructions, "Answer only in haiku.");
    assert!(!installed.instructions.contains("---"));
    // A fleet that has never checkpointed joins the LEFT side of the join and
    // gets the sentinel, not a NULL the caller has to think about.
    assert_eq!(installed.context_json, FRESH_CONTEXT);
    assert!(installed.bundle_content_hash.is_none());
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_stopped_fleet_is_not_runnable_and_is_not_an_error() {
    // The window this closes: the selection pass filters on `active`, so
    // reaching here with a stopped fleet means an operator paused it in
    // between. That is not a fault to report — it is the operator getting what
    // they asked for.
    let fixtures = Fixtures::create_with_queue().await;
    let Seeded { fleet, .. } = seeded::<0>(&fixtures).await;
    install(&fixtures, &fleet, STORED_CONFIG, "paused").await;

    let installed = fixtures
        .leases()
        .installed(&id(&fleet))
        .await
        .expect("a stopped fleet is not a datastore failure");

    assert!(installed.is_none());
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_fleet_with_no_row_is_not_runnable_and_is_not_an_error() {
    let fixtures = Fixtures::create_with_queue().await;
    // A well-formed identifier that no row carries — the read must answer
    // "nothing to run" rather than treating an unknown fleet as a fault.
    let absent = Uuid7::parse("01920000-0000-7000-8000-00000000dead")
        .expect("the fixture identifier is a uuid");

    let installed = fixtures
        .leases()
        .installed(&absent)
        .await
        .expect("a missing fleet is not a datastore failure");

    assert!(installed.is_none());
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_an_unreadable_config_refuses_permanently_rather_than_retrying() {
    // The one answer that is an error, and the classification is the point: a
    // document that will not parse will not parse on the next poll either, so
    // the delivery must reach a terminal row instead of staying leasable
    // forever while a warn line repeats at the poll interval.
    let fixtures = Fixtures::create_with_queue().await;
    let Seeded { fleet, .. } = seeded::<0>(&fixtures).await;
    install(&fixtures, &fleet, r#"{"not":"a fleet config"}"#, "active").await;

    let failure = fixtures
        .leases()
        .installed(&id(&fleet))
        .await
        .expect_err("a config this daemon cannot read must not run");

    assert!(failure.is_config_permanent(), "{failure}");
    assert!(!failure.is_datastore_unavailable(), "{failure}");
}
