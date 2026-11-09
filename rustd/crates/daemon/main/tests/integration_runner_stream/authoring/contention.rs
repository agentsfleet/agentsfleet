//! One report identity survives contending callers within the native stream's limits.

use super::fixture::AuthoringFixture;
use agentsfleetd::supervisor::Supervisor;
use futures_util::{StreamExt as _, stream};
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::{Barrier, Semaphore};

const CALLERS: usize = 100;
const CONCURRENT_REPORTS: usize = 4;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn contending_completion_retries_share_one_saved_acknowledgement_and_charge() {
    let mut supervisor = Supervisor::new();
    let fixture = AuthoringFixture::start(&mut supervisor, false).await;
    let lease = fixture.id();
    fixture
        .session
        .authoring_model(&lease, &fixture.model())
        .await
        .expect("one physical model call");
    let report = fixture.report(1);
    let barrier = Barrier::new(CALLERS);
    let slots = Semaphore::new(CONCURRENT_REPORTS);
    let active = AtomicUsize::new(0);
    let peak = AtomicUsize::new(0);
    let results = stream::iter(0..CALLERS)
        .map(|_| async {
            barrier.wait().await;
            let _slot = slots.acquire().await.expect("bounded native concurrency");
            let running = active.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(running, Ordering::SeqCst);
            let ack = fixture
                .session
                .authoring_report(&lease, &report)
                .await
                .expect("contending completion");
            active.fetch_sub(1, Ordering::SeqCst);
            assert!(ack.settled);
            ack.replayed
        })
        .buffer_unordered(CALLERS)
        .collect::<Vec<_>>()
        .await;
    assert_eq!(results.iter().filter(|replayed| !**replayed).count(), 1);
    assert!(peak.load(Ordering::SeqCst) > 1, "requests overlapped");
    assert_eq!(fixture.receipts().await, (1, 0, 1));
    assert_eq!(fixture.charges().await.len(), 2);
    assert_eq!(fixture.provider.state.count(), 1);
    fixture.finish(supervisor).await;
}
