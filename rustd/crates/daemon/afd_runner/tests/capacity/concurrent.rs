//! A second claimant sees the first commit after waiting for the host lock.

use super::Fixture;
use std::sync::Arc;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_concurrent_claims_share_one_worker_limit() {
    let fixture = Fixture::create(1).await;
    let shared = &fixture;
    let barrier = Arc::new(tokio::sync::Barrier::new(2));
    let claim = |barrier: Arc<tokio::sync::Barrier>| async move {
        barrier.wait().await;
        shared.try_claim().await
    };
    let (first, second) = tokio::join!(claim(Arc::clone(&barrier)), claim(barrier));
    assert_ne!(first, second, "only one replica can acquire the last slot");
    assert!(
        !fixture.try_claim().await,
        "another poll still sees the committed slot"
    );
    fixture.cleanup().await;
}
