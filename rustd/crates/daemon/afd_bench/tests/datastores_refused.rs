//! A run whose datastores answer nothing is refused before it measures, and
//! the refusal names which store to start.
//!
//! Port 1 is reserved and unbound on every platform this builds for, so a dial
//! fails on refusal in microseconds. Postgres is dialled first, so the case
//! that reaches the Bus needs a live one.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use af_core::error_code;
use afd_bench::datastores::Datastores;
use afd_db::test_util::TestDatabase;

/// A Postgres URL whose port nothing listens on.
const NOWHERE_DATABASE: &str = "postgres://nobody:nobody@127.0.0.1:1/nothing?sslmode=disable";

/// NATS seeds whose port nothing listens on.
const NOWHERE_NATS: &str = "nats://127.0.0.1:1";

#[tokio::test]
async fn test_a_run_that_cannot_reach_its_database_is_refused_as_the_database() {
    let refused = Datastores::open(NOWHERE_DATABASE, NOWHERE_NATS)
        .await
        .expect_err("a datastore nobody listens on is refused, not measured");

    assert_eq!(
        refused.code(),
        error_code::INTERNAL_DB_UNAVAILABLE,
        "the refusal says to start Postgres: {refused}"
    );
}

#[tokio::test]
#[ignore = "needs live Postgres: run by hand with `cargo test -p afd_bench --all-features -- --ignored` under the lane's environment"]
async fn test_a_run_whose_bus_will_not_answer_is_refused_as_the_bus() {
    let database = TestDatabase::shared();

    let refused = Datastores::open(&database.url(), NOWHERE_NATS)
        .await
        .expect_err("a Bus nobody listens on is refused, not measured");

    assert_eq!(
        refused.code(),
        error_code::STARTUP_BUS_CONNECT,
        "the refusal says to start NATS, not Postgres: {refused}"
    );
}
