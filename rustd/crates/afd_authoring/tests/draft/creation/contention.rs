//! Duplicate requests contend on real rows and converge on one retained identity.

use super::super::{fixture::now, mint};
use super::fixture::Case;
use afd_authoring::types::Start;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::sync::Barrier;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn one_hundred_concurrent_saves_converge_on_one_retained_fleet() {
    const CALLERS: usize = 100;
    let c = Arc::new(Case::create().await);
    let (_, request) = c.request("contended-save", Start::None).await;
    let key = mint();
    let barrier = Arc::new(Barrier::new(CALLERS));
    let entered = Arc::new(AtomicUsize::new(0));
    let mut tasks = Vec::new();
    for _ in 0..CALLERS {
        let (c, request, key, barrier, entered) = (
            Arc::clone(&c),
            request.clone(),
            key.clone(),
            Arc::clone(&barrier),
            Arc::clone(&entered),
        );
        tasks.push(tokio::spawn(async move {
            entered.fetch_add(1, Ordering::SeqCst);
            barrier.wait().await;
            // Every caller waits its turn on the key and resumes the same fleet.
            c.creations
                .save(c.base.access(), &key, request.clone(), now())
                .await
                .expect("a concurrent caller resumes")
        }));
    }
    let mut identities = Vec::new();
    for task in tasks {
        identities.push(task.await.expect("request task").id);
    }
    assert_eq!(entered.load(Ordering::SeqCst), CALLERS);
    assert_eq!(identities.len(), CALLERS);
    assert!(identities.iter().all(|id| id == &identities[0]));
    assert_eq!(c.fleet_count().await, 1);
    Arc::try_unwrap(c)
        .ok()
        .expect("all request clones dropped")
        .cleanup()
        .await;
}
