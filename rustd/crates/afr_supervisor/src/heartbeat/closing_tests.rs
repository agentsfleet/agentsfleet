//! A runner that takes no new lease ends every hold and tells the daemon at
//! once, whether leasing alone stopped or the runner is shutting down.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use afd_core::test_util::trace::Capture;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use super::Assignment;
use super::Heartbeat;
use super::tests::{
    FLEET, HOLDS_FIELD, KEEP_GOING, RELEASED, STOP, TICK_MS, holding, probe, releasing, sent,
};
use crate::client::Call;
use crate::halt::Halt;
use crate::holds::Release;
use crate::test_support::{Answer, FakeEngine, drain, plane};

/// Every hold the beat ended, by the reason it was logged under.
fn reasons(capture: &Capture) -> Vec<String> {
    capture
        .events()
        .iter()
        .filter(|event| event.field("event") == Some(RELEASED))
        .map(|event| event.field("reason").unwrap().to_owned())
        .collect()
}

/// The holds each beat listed, in the order the daemon received them.
fn listed(calls: &[Call]) -> Vec<serde_json::Value> {
    calls
        .iter()
        .map(|call| sent(call)[HOLDS_FIELD].clone())
        .collect()
}

/// Leasing stops while the runner serves on: every hold ends, since no lease
/// could take one, and the next beat goes at once listing none, rather than
/// at the tick, so the daemon stops routing those fleets here.
#[tokio::test(start_paused = true)]
async fn test_leasing_stopped_ends_every_hold_and_beats_at_once() {
    let capture = Capture::install();
    let engine = FakeEngine::default();
    let holds = holding(&engine, 2).await;
    let beats = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&beats);
    let (plane, mut calls) = plane(move |_call| match counted.fetch_add(1, Ordering::SeqCst) {
        0 => releasing(KEEP_GOING, &[]),
        _later => releasing(STOP, &[]),
    });
    let probe = probe();
    let (published, mut watching) = watch::channel(Assignment::initial());
    let halt = Halt::new(CancellationToken::new());
    let started = tokio::time::Instant::now();

    let beating = Heartbeat::new(&plane, &probe, &holds).keep_beating(&published, &halt);
    let stopping = async {
        watching.changed().await.unwrap();
        halt.stop_leasing();
    };
    let ((), ()) = tokio::join!(beating, stopping);
    holds.shutdown().await;

    assert!(
        started.elapsed() < Duration::from_millis(u64::from(TICK_MS)),
        "the beat after leasing stopped did not wait out the tick"
    );
    let listed = listed(&drain(&mut calls));
    assert_eq!(listed[0], serde_json::json!([FLEET]));
    assert_eq!(listed[1], serde_json::json!([]), "{listed:?}");
    let shutdown = Release::Shutdown.outcome().as_str();
    assert_eq!(reasons(&capture), [shutdown]);
    assert_eq!(engine.destroyed.load(Ordering::SeqCst), 1);
}

/// A runner shutting down ends every hold and says so in one last beat, so
/// the daemon routes those fleets elsewhere now rather than once it finds
/// this runner gone.
#[tokio::test(start_paused = true)]
async fn test_a_shutdown_ends_every_hold_and_says_so_in_a_last_beat() {
    let capture = Capture::install();
    let engine = FakeEngine::default();
    let holds = holding(&engine, 2).await;
    let (plane, mut calls) = plane(|_call| releasing(KEEP_GOING, &[]));
    let probe = probe();
    let (published, mut watching) = watch::channel(Assignment::initial());
    let shutdown = CancellationToken::new();
    let halt = Halt::new(shutdown.clone());

    let beating = Heartbeat::new(&plane, &probe, &holds).keep_beating(&published, &halt);
    let stopping = async {
        watching.changed().await.unwrap();
        shutdown.cancel();
    };
    let ((), ()) = tokio::join!(beating, stopping);
    let left = holds.fleets().await;
    holds.shutdown().await;

    assert!(left.is_empty(), "{left:?}");
    let listed = listed(&drain(&mut calls));
    assert_eq!(
        listed,
        [serde_json::json!([FLEET]), serde_json::json!([])],
        "the beat, then the last one listing nothing"
    );
    let ended = Release::Shutdown.outcome().as_str();
    assert_eq!(reasons(&capture), [ended]);
    assert_eq!(engine.destroyed.load(Ordering::SeqCst), 1);
}

/// A runner whose token was refused sends no last beat: no call can succeed.
#[tokio::test(start_paused = true)]
async fn test_a_refused_runner_sends_no_last_beat() {
    let engine = FakeEngine::default();
    let holds = holding(&engine, 2).await;
    let (plane, mut calls) =
        plane(|call| Answer::Fail(crate::error::refused(call.verb, 401, None)));
    let probe = probe();
    let (published, _watching) = watch::channel(Assignment::initial());
    let halt = Halt::new(CancellationToken::new());

    Heartbeat::new(&plane, &probe, &holds)
        .keep_beating(&published, &halt)
        .await;

    assert!(halt.token_refused());
    assert_eq!(drain(&mut calls).len(), 1, "the refused beat alone");
}
