#![expect(
    clippy::unwrap_used,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use afd_core::limits::WorkerCount;
use afd_wire::lease::LeaseResponse;
use afd_wire::memory::MemoryHydrateResponse;
use afd_wire::runner::HeartbeatStatus;
use afr_sandbox::Limits;
use bytes::Bytes;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use super::serve;
use crate::bundles::BundleCache;
use crate::client::{Call, Verb};
use crate::error;
use crate::heartbeat::Assignment;
use crate::lease_loop::Lessee;
use crate::report_spool::ReportSpool;
use crate::storage_home::StorageHome;
use crate::test_support::{Answer, Behaviour, FLEET_ID, FakeAgent, FakeEngine, json, lease, plane};

const OTHER_LEASE: &str = "01890a5d-ac96-774b-bcce-b302099a8060";
const SECOND_LEASE: &str = "01890a5d-ac96-774b-bcce-b302099a8061";

/// A daemon whose n-th lease poll is answered by `polls(n)`.
fn daemon(
    polls: impl Fn(usize) -> Answer + Send + Sync + 'static,
) -> impl Fn(&Call) -> Answer + Send + Sync + 'static {
    let polled = AtomicUsize::new(0);
    move |call| match call.verb {
        Verb::Lease => polls(polled.fetch_add(1, Ordering::SeqCst)),
        Verb::Hydrate => json(&MemoryHydrateResponse { memory: Vec::new() }),
        _ => json(&serde_json::json!({"ok": true, "stored": 0, "skipped": 0})),
    }
}

fn idle() -> Answer {
    json(&LeaseResponse {
        lease: None,
        retry_after_ms: Some(500),
    })
}

fn lessee(
    answer: impl Fn(&Call) -> Answer + Send + Sync + 'static,
    agent: FakeAgent,
    home: &StorageHome,
) -> Arc<Lessee> {
    Arc::new(Lessee {
        plane: plane(answer).0,
        engine: Box::new(FakeEngine::default()),
        agent: Box::new(agent),
        spool: ReportSpool::new(home),
        bundles: BundleCache::new(home),
        limits: Limits::default(),
    })
}

fn assigned(workers: u32) -> Assignment {
    Assignment {
        workers: WorkerCount::clamping(workers),
        ..Assignment::initial()
    }
}

#[tokio::test(start_paused = true)]
async fn test_worker_pool_runs_distinct_fleets() {
    let root = tempfile::tempdir().unwrap();
    let home = StorageHome::open(root.path()).unwrap();
    let agent = FakeAgent::new(Behaviour::Answer);
    let (runs, peak) = (Arc::clone(&agent.runs), Arc::clone(&agent.peak));
    let answer = daemon(|polled| match polled {
        0 => json(&LeaseResponse {
            lease: Some(lease(OTHER_LEASE, FLEET_ID, None)),
            retry_after_ms: None,
        }),
        1 => json(&LeaseResponse {
            lease: Some(lease(SECOND_LEASE, FLEET_ID, None)),
            retry_after_ms: None,
        }),
        _ => idle(),
    });
    let (_published, watching) = watch::channel(assigned(2));
    let shutdown = CancellationToken::new();
    let pool = tokio::spawn(serve(
        lessee(answer, agent, &home),
        watching,
        shutdown.clone(),
    ));

    while runs.load(Ordering::SeqCst) < 2 {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    tokio::time::sleep(Duration::from_secs(1)).await;
    shutdown.cancel();
    pool.await.unwrap();

    assert_eq!(
        runs.load(Ordering::SeqCst),
        2,
        "both of the fleet's events ran"
    );
    assert_eq!(peak.load(Ordering::SeqCst), 1, "never at the same time");
}

#[tokio::test(start_paused = true)]
async fn a_refused_token_stops_the_pool() {
    let root = tempfile::tempdir().unwrap();
    let home = StorageHome::open(root.path()).unwrap();
    let answer = daemon(|_| Answer::Fail(error::refused(Verb::Lease, 401, None)));
    let (_published, watching) = watch::channel(assigned(1));
    let shutdown = CancellationToken::new();

    serve(
        lessee(answer, FakeAgent::new(Behaviour::Answer), &home),
        watching,
        shutdown.clone(),
    )
    .await;

    assert!(shutdown.is_cancelled());
}

#[tokio::test(start_paused = true)]
async fn the_pool_grows_with_its_assignment_and_rides_out_bad_polls() {
    let root = tempfile::tempdir().unwrap();
    let home = StorageHome::open(root.path()).unwrap();
    let polls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&polls);
    let answer = daemon(move |polled| {
        counted.fetch_add(1, Ordering::SeqCst);
        match polled {
            0 => Answer::Fail(error::unavailable(Verb::Lease, 503)),
            1 => Answer::Reply(Bytes::from_static(b"[")),
            2 => json(&LeaseResponse {
                lease: Some(lease("nope", FLEET_ID, None)),
                retry_after_ms: None,
            }),
            _ => json(&LeaseResponse {
                lease: None,
                retry_after_ms: None,
            }),
        }
    });
    let (published, watching) = watch::channel(assigned(1));
    let pool = tokio::spawn(serve(
        lessee(answer, FakeAgent::new(Behaviour::Answer), &home),
        watching,
        CancellationToken::new(),
    ));

    tokio::time::sleep(Duration::from_secs(5)).await;
    published.send_replace(assigned(2));
    tokio::time::sleep(Duration::from_secs(5)).await;
    // Draining, then gone: no worker is wanted and no assignment can come.
    published.send_replace(Assignment {
        status: HeartbeatStatus::Drain,
        ..assigned(2)
    });
    drop(published);
    pool.await.unwrap();

    assert!(
        polls.load(Ordering::SeqCst) > 4,
        "every bad poll was ridden out"
    );
}
