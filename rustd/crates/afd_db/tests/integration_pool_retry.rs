//! The retry a stalled acquire gets, and the name it fails under.
//!
//! `integration_pool_faults.rs` proves a pool that loses its datastore does not
//! report capacity. These prove what it reports instead — a stall, with the
//! census — and that a stall is tried twice before it is answered, because the
//! second attempt is the whole point of naming it: a stall is the one acquire
//! failure the next budget can change.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::Duration;

use afd_core::error_code;
use afd_db::Db;
use afd_db::config::DbRole;

use super::integration_pool_faults::fault_net::{FaultProxy, install_subscriber};
use super::integration_pool_faults::lane::{
    ACQUIRE_BUDGET_MS, config_through, lane_database, lane_target,
};

/// A stall that a second attempt does not clear is answered as a stall, with
/// the census, after exactly two budgets.
///
/// Two, not one: the elapsed time is the proof that the retry ran, because a
/// retry that was skipped and a retry that failed produce the same error. And
/// not more than two: a retry that retried would turn one slow 503 into a hang.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_stalled_acquire_is_retried_once_then_named() {
    install_subscriber();
    let proxy = FaultProxy::to(lane_target()).await;
    let db = Db::connect(&config_through(proxy.addr(), DbRole::Api, &lane_database()))
        .await
        .expect("the proxy relays, so the connect must succeed");
    proxy.swallow();
    let budget = Duration::from_millis(ACQUIRE_BUDGET_MS);

    let started = tokio::time::Instant::now();
    let error = db
        .acquire()
        .await
        .expect_err("nothing answers, so nothing can be acquired");
    let elapsed = started.elapsed();

    assert!(error.is_acquire_stalled(), "{error}");
    assert!(
        error.is_datastore_unavailable(),
        "a stall is still a request with no datastore: {error}"
    );
    assert!(
        !error.is_pool_capacity(),
        "nothing was ever opened to run out of: {error}"
    );
    assert_eq!(error.code(), error_code::INTERNAL_DB_UNAVAILABLE);
    // The census: a lazy pool that could not open its first connection held
    // nothing, and the message says so rather than calling it an outage.
    assert!(
        error.to_string().contains("held 0 of "),
        "the message must carry the census: {error}"
    );
    assert!(
        elapsed >= budget * 2,
        "one budget spent means the retry never ran: {elapsed:?}"
    );
    // Generous on the upper side: this asserts how many budgets applied, not
    // how precise a timer is on a loaded machine.
    assert!(
        elapsed < budget * 6,
        "more than two budgets spent means something retried the retry: {elapsed:?}"
    );

    db.close().await;
}

/// The retry is worth having: a datastore that answers again inside the
/// second budget turns a would-be 503 into a connection.
///
/// The proxy relays again halfway through the FIRST attempt. That attempt is
/// already parked on a socket the proxy will never answer, so it still expires
/// on its budget; the retry is what meets a datastore that answers, and the
/// elapsed time proves the success was the retry's and not the first try's.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_retry_that_finds_the_datastore_back_succeeds() {
    install_subscriber();
    let proxy = FaultProxy::to(lane_target()).await;
    let db = Db::connect(&config_through(proxy.addr(), DbRole::Api, &lane_database()))
        .await
        .expect("the proxy relays, so the connect must succeed");
    proxy.swallow();
    let budget = Duration::from_millis(ACQUIRE_BUDGET_MS);

    let started = tokio::time::Instant::now();
    let (acquired, ()) = tokio::join!(db.acquire(), async {
        tokio::time::sleep(budget / 2).await;
        proxy.relay();
    });
    let elapsed = started.elapsed();

    let connection = acquired.expect("the retry ran against a proxy that relays again");
    assert!(
        elapsed >= budget,
        "a success inside the first budget was not the retry's: {elapsed:?}"
    );

    drop(connection);
    db.close().await;
}
