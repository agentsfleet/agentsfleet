//! The queued history index remains available after receipt removal.
#![cfg(feature = "test-util")]
#![expect(clippy::expect_used, clippy::panic, reason = "test assertions")]
use crate::support::Fixtures;

/// The index the delivery stamp is served by.
const LOOKUP_INDEX: &str = "idx_admissions_delivery_lookup";

/// The columns the stamp keys on, in the order it keys on them.
const LOOKUP_COLUMNS: &str = "(fleet_id, created_at, id)";

/// The predicate the stamp's own `WHERE` implies, and the whole of it.
const LOOKUP_PREDICATE: &str = "WHERE (delivered_at IS NULL)";

/// The clause that must NOT appear: it is what made the old index unusable.
const RECEIPT_CLAUSE: &str = "receipt";

/// The index's definition as PostgreSQL stores it.
async fn definition(fixtures: &Fixtures, index: &str) -> String {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("the ledger answers");
    sqlx::query_scalar("SELECT indexdef FROM pg_indexes WHERE indexname = $1")
        .bind(index)
        .fetch_one(&mut *connection)
        .await
        .unwrap_or_else(|failure| panic!("{index} must exist after migration: {failure}"))
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn the_delivery_lookup_index_matches_the_queued_history_predicate() {
    let fixtures = Fixtures::create().await;
    let lookup = definition(&fixtures, LOOKUP_INDEX).await;
    assert!(lookup.contains(LOOKUP_COLUMNS), "{lookup}");
    assert!(lookup.contains(LOOKUP_PREDICATE), "{lookup}");
    assert!(!lookup.contains(RECEIPT_CLAUSE), "{lookup}");
    fixtures.cleanup().await;
}
