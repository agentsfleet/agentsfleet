//! A run whose datastores answer nothing is refused before it measures.
//!
//! Port 1 is reserved and unbound on every platform this builds for, so both
//! dials fail on refusal in microseconds and the test needs no datastore.

/// A Postgres URL whose port nothing listens on.
const NOWHERE_DATABASE: &str = "postgres://nobody:nobody@127.0.0.1:1/nothing?sslmode=disable";

/// A Dragonfly URL whose port nothing listens on.
const NOWHERE_DRAGONFLY: &str = "redis://127.0.0.1:1";

#[tokio::test]
async fn test_a_run_that_cannot_reach_its_datastore_writes_no_result() {
    let refused =
        afd_bench::datastores::Datastores::open(NOWHERE_DATABASE, NOWHERE_DRAGONFLY, None).await;

    assert!(
        refused.is_err(),
        "a datastore nobody listens on is refused, not measured"
    );
}
