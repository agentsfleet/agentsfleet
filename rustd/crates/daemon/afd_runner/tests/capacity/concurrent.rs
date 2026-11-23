//! A second assignment sees the first commit after waiting for the host lock.

use super::Fixture;
use std::sync::Arc;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_concurrent_assignments_share_one_worker_limit() {
    let fixture = Fixture::create(1).await;
    let shared = &fixture;
    let barrier = Arc::new(tokio::sync::Barrier::new(2));
    let assign = async |barrier: Arc<tokio::sync::Barrier>| {
        barrier.wait().await;
        shared.try_assign().await
    };
    let (first, second) = tokio::join!(assign(Arc::clone(&barrier)), assign(barrier));
    assert_ne!(
        first, second,
        "only one replica can acquire the last worker"
    );
    assert!(
        !fixture.try_assign().await,
        "another poll still sees the committed worker"
    );
    fixture.cleanup().await;
}
