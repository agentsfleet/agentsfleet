//! Busy/held handoffs and cancellation retain one charge per sandbox.

#![expect(clippy::unwrap_used, reason = "a failed fixture fails the test")]

use std::sync::Arc;

use futures_util::FutureExt as _;
use tokio::sync::Barrier;

use super::tests::{FLEET_A, FLEET_B, RUN, fleets, hold, holds, id, key, sandbox};
use super::{Occupant, Parked};
use crate::test_support::FakeEngine;

#[tokio::test]
async fn test_a_hold_never_takes_the_last_free_worker() {
    let engine = FakeEngine::default();
    let (holds, _clock) = holds(2);
    hold(&holds, &engine, FLEET_A).await.unwrap();
    hold(&holds, &engine, FLEET_B).await.unwrap();
    assert_eq!(fleets(&holds).await, [FLEET_B], "oldest retires first");
    assert!(holds.saturated().notified().now_or_never().is_some());
    holds.resize(1);
    assert!(
        fleets(&holds).await.is_empty(),
        "a single worker keeps no holds"
    );
    holds.shutdown().await;
}

#[tokio::test]
async fn test_two_worker_hold_transfers_count_each_sandbox_once() {
    let engine = FakeEngine::default();
    let (holds, _clock) = holds(2);
    hold(&holds, &engine, FLEET_A).await.unwrap();
    let guard = holds.occupy(Occupant::Fleet {
        run: id(RUN),
        fleet: id(FLEET_A),
    });
    assert_eq!(
        fleets(&holds).await,
        [FLEET_A],
        "occupy protects its own handoff"
    );
    let taken = holds.take(&key(FLEET_A)).await.unwrap();
    assert!(matches!(
        holds.park(key(FLEET_A), id(RUN), taken.sandbox).await,
        Parked::Held(_)
    ));
    drop(guard);
    assert_eq!(
        fleets(&holds).await,
        [FLEET_A],
        "drop after park never double-vacates"
    );
    holds.shutdown().await;
}

#[tokio::test]
async fn test_setup_occupancy_preserves_the_keeper_reserve() {
    let engine = FakeEngine::default();
    let (holds, _clock) = holds(2);
    hold(&holds, &engine, FLEET_A).await.unwrap();
    let setup = holds.occupy(Occupant::Setup { run: id(FLEET_B) });
    assert_eq!(fleets(&holds).await, Vec::<String>::new());
    drop(setup);
    assert!(hold(&holds, &engine, FLEET_A).await.is_some());
    holds.shutdown().await;
}

#[tokio::test]
async fn cancelled_park_and_guard_drop_leave_no_busy_reservation() {
    let engine = FakeEngine::default();
    let (holds, _clock) = holds(2);
    let busy = holds.occupy(Occupant::Fleet {
        run: id(RUN),
        fleet: id(FLEET_A),
    });
    let frozen = sandbox(&engine).await;
    assert!(
        holds
            .park(key(FLEET_A), id(RUN), frozen)
            .now_or_never()
            .is_none()
    );
    drop(busy);
    assert!(
        fleets(&holds).await.is_empty(),
        "cancelled park cannot leave a hold"
    );
    assert!(hold(&holds, &engine, FLEET_B).await.is_some());
    holds.shutdown().await;
}

#[tokio::test]
async fn concurrent_setup_guard_drops_release_only_their_own_runs() {
    let engine = FakeEngine::default();
    let (holds, _clock) = holds(2);
    let keeper = holds.occupy(Occupant::Setup { run: id(RUN) });
    let barrier = Arc::new(Barrier::new(100));
    let mut workers = tokio::task::JoinSet::new();
    for _ in 0..100 {
        let holds = holds.clone();
        let barrier = Arc::clone(&barrier);
        workers.spawn(async move {
            let run = af_crypto::entropy::Entropy::new()
                .uuid7(af_core::clock::now())
                .unwrap();
            let guard = holds.occupy(Occupant::Setup { run });
            barrier.wait().await;
            drop(guard);
        });
    }
    while let Some(worker) = workers.join_next().await {
        worker.unwrap();
    }
    assert!(
        hold(&holds, &engine, FLEET_A).await.is_none(),
        "the original setup run still owns a worker"
    );
    drop(keeper);
    assert!(hold(&holds, &engine, FLEET_A).await.is_some());
    holds.shutdown().await;
}

#[tokio::test]
async fn accepted_hold_deadlines_only_shorten_the_exact_parked_attempt() {
    use std::time::Duration;
    let engine = FakeEngine::default();
    let (holds, clock) = holds(2);
    let until = hold(&holds, &engine, FLEET_A).await.unwrap();
    let shorter = af_core::clock::after(af_core::clock::Clock::now(&clock), Duration::from_secs(5));
    holds.accept(id(FLEET_B), shorter);
    assert_eq!(
        fleets(&holds).await,
        [FLEET_A],
        "another attempt cannot shorten this hold"
    );
    holds.accept(id(RUN), shorter);
    holds.accept(id(RUN), until);
    clock.advance(Duration::from_secs(5));
    assert_eq!(
        fleets(&holds).await,
        Vec::<String>::new(),
        "an acknowledgement cannot extend the deadline again"
    );
    holds.shutdown().await;
}
