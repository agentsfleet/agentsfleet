//! Concurrent web dispatches share the production clients and connection pools.
#![expect(
    clippy::expect_used,
    reason = "failed construction invalidates the test"
)]

use super::Web;

#[tokio::test]
async fn concurrent_dispatches_initialize_one_pair_of_vendor_clients() {
    let web = Web::default();
    let (first, second) = tokio::join!(web.clients(), web.clients());
    assert!(std::ptr::eq(
        first.expect("guarded vendor clients"),
        second.expect("same guarded vendor clients")
    ));
}
