//! How the holds end when the runner stops: every hold destroyed and waited
//! for, a park after the stop handed back, and a second stop answered at once.

#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on a fixture it cannot build"
)]

use std::sync::atomic::Ordering;
use std::time::Duration;

use afd_core::test_util::trace::Capture;

use super::tests::{FLEET_A, FLEET_B, LEASE, holds, id, key, park, released, releases, sandbox};
use super::{Parked, Release};
use crate::test_support::FakeEngine;

#[tokio::test]
async fn test_shutdown_destroys_every_hold_and_waits() {
    let capture = Capture::install();
    let engine = FakeEngine::default();
    let (holds, _clock) = holds(2);
    park(&holds, &engine, FLEET_A).await;
    park(&holds, &engine, FLEET_B).await;

    holds.shutdown().await;

    assert_eq!(engine.destroyed.load(Ordering::SeqCst), 2);
    assert_eq!(
        releases(&capture),
        [
            released(FLEET_A, Release::Shutdown),
            released(FLEET_B, Release::Shutdown)
        ]
    );
}

/// A park after the keeper stopped hands the sandbox back to the lease, which
/// kills it, reports, and only then releases it; the keeper destroys
/// nothing itself.
#[tokio::test]
async fn test_a_park_after_shutdown_hands_the_sandbox_back() {
    let capture = Capture::install();
    let engine = FakeEngine::default();
    let (holds, _clock) = holds(2);
    holds.shutdown().await;

    let parked = holds
        .park(key(FLEET_A), id(LEASE), sandbox(&engine).await)
        .await;

    let handed_back = matches!(parked, Parked::Refused(_));
    let destroyed_by_the_keeper = engine.destroyed.load(Ordering::SeqCst);
    if let Parked::Refused(sandbox) = parked {
        sandbox.destroy().await.unwrap();
    }
    assert!(handed_back, "a stopped keeper hands the sandbox back");
    assert_eq!(destroyed_by_the_keeper, 0, "and destroys nothing itself");
    assert_eq!(releases(&capture), [released(FLEET_A, Release::Shutdown)]);
    let left = holds.fleets().await;
    assert!(left.is_empty(), "{left:?}");
}

#[tokio::test]
async fn test_a_second_shutdown_returns_at_once() {
    let engine = FakeEngine::default();
    let (holds, _clock) = holds(2);
    park(&holds, &engine, FLEET_A).await;
    holds.shutdown().await;

    // The task is gone, so nothing is left to answer; waiting for it would
    // hang a runner whose exit path calls this twice.
    tokio::time::timeout(Duration::from_secs(1), holds.shutdown())
        .await
        .unwrap();

    assert_eq!(
        engine.destroyed.load(Ordering::SeqCst),
        1,
        "nothing is destroyed twice"
    );
}
