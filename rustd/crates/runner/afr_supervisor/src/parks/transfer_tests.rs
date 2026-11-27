//! Busy/parked handoffs and cancellation retain one charge per sandbox.

#![expect(clippy::unwrap_used, reason = "a failed fixture fails the test")]

use std::sync::Arc;

use futures_util::FutureExt as _;
use tokio::sync::Barrier;

use super::tests::{FLEET_A, FLEET_B, RUN, fleets, id, key, park_until, parks, sandbox};
use super::{Occupant, Stowed};
use crate::test_support::FakeEngine;

#[tokio::test]
async fn test_a_park_never_takes_the_last_free_worker() {
    let engine = FakeEngine::default();
    let (parks, _clock) = parks(2);
    park_until(&parks, &engine, FLEET_A).await.unwrap();
    park_until(&parks, &engine, FLEET_B).await.unwrap();
    assert_eq!(fleets(&parks).await, [FLEET_B], "oldest retires first");
    assert!(parks.saturated().notified().now_or_never().is_some());
    parks.resize(1);
    assert!(
        fleets(&parks).await.is_empty(),
        "a single worker keeps no parks"
    );
    parks.shutdown().await;
}

#[tokio::test]
async fn test_two_worker_park_transfers_count_each_sandbox_once() {
    let engine = FakeEngine::default();
    let (parks, _clock) = parks(2);
    park_until(&parks, &engine, FLEET_A).await.unwrap();
    let guard = parks.occupy(Occupant {
        run: id(RUN),
        fleet: id(FLEET_A),
    });
    assert_eq!(
        fleets(&parks).await,
        [FLEET_A],
        "occupy protects its own handoff"
    );
    let taken = parks.take(&key(FLEET_A)).await.unwrap();
    assert!(matches!(
        parks.park(key(FLEET_A), id(RUN), taken.sandbox).await,
        Stowed::Parked(_)
    ));
    drop(guard);
    assert_eq!(
        fleets(&parks).await,
        [FLEET_A],
        "drop after park never double-vacates"
    );
    parks.shutdown().await;
}

/// Another fleet's run takes a worker, so a park it cannot transfer into is
/// released to keep the reserve.
#[tokio::test]
async fn test_another_fleets_occupancy_preserves_the_keeper_reserve() {
    let engine = FakeEngine::default();
    let (parks, _clock) = parks(2);
    park_until(&parks, &engine, FLEET_A).await.unwrap();
    let other = parks.occupy(Occupant {
        run: id(RUN),
        fleet: id(FLEET_B),
    });
    assert_eq!(fleets(&parks).await, Vec::<String>::new());
    drop(other);
    assert!(park_until(&parks, &engine, FLEET_A).await.is_some());
    parks.shutdown().await;
}

#[tokio::test]
async fn cancelled_park_and_guard_drop_leave_no_busy_reservation() {
    let engine = FakeEngine::default();
    let (parks, _clock) = parks(2);
    let busy = parks.occupy(Occupant {
        run: id(RUN),
        fleet: id(FLEET_A),
    });
    let parked = sandbox(&engine).await;
    assert!(
        parks
            .park(key(FLEET_A), id(RUN), parked)
            .now_or_never()
            .is_none()
    );
    drop(busy);
    assert!(
        fleets(&parks).await.is_empty(),
        "a cancelled park leaves no sandbox parked"
    );
    assert!(park_until(&parks, &engine, FLEET_B).await.is_some());
    parks.shutdown().await;
}

#[tokio::test]
async fn concurrent_guard_drops_release_only_their_own_runs() {
    let engine = FakeEngine::default();
    let (parks, _clock) = parks(2);
    let keeper = parks.occupy(Occupant {
        run: id(RUN),
        fleet: id(FLEET_B),
    });
    let barrier = Arc::new(Barrier::new(100));
    let mut workers = tokio::task::JoinSet::new();
    for _ in 0..100 {
        let parks = parks.clone();
        let barrier = Arc::clone(&barrier);
        workers.spawn(async move {
            let run = af_crypto::entropy::Entropy::new()
                .uuid7(af_core::clock::now())
                .unwrap();
            let guard = parks.occupy(Occupant {
                run,
                fleet: id(FLEET_B),
            });
            barrier.wait().await;
            drop(guard);
        });
    }
    while let Some(worker) = workers.join_next().await {
        worker.unwrap();
    }
    assert!(
        park_until(&parks, &engine, FLEET_A).await.is_none(),
        "the original run still owns a worker"
    );
    drop(keeper);
    assert!(park_until(&parks, &engine, FLEET_A).await.is_some());
    parks.shutdown().await;
}

#[tokio::test]
async fn accepted_park_deadlines_only_shorten_the_exact_parked_attempt() {
    use std::time::Duration;
    let engine = FakeEngine::default();
    let (parks, clock) = parks(2);
    let until = park_until(&parks, &engine, FLEET_A).await.unwrap();
    let shorter = af_core::clock::after(af_core::clock::Clock::now(&clock), Duration::from_secs(5));
    parks.accept(id(FLEET_B), shorter);
    assert_eq!(
        fleets(&parks).await,
        [FLEET_A],
        "another attempt cannot shorten this park"
    );
    parks.accept(id(RUN), shorter);
    parks.accept(id(RUN), until);
    clock.advance(Duration::from_secs(5));
    assert_eq!(
        fleets(&parks).await,
        Vec::<String>::new(),
        "an acknowledgement cannot extend the deadline again"
    );
    parks.shutdown().await;
}
