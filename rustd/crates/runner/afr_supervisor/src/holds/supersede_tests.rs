//! A superseded report ends only the hold its own run parked.

#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on a fixture it cannot build"
)]

use std::sync::atomic::Ordering;

use af_core::test_util::trace::Capture;

use super::Release;
use super::tests::{FLEET_A, RUN, fleets, holds, id, key, released, releases, sandbox};
use crate::test_support::{FakeEngine, bounded};

/// The fleet's next run, which took the hold and parked it again.
const NEXT_RUN: &str = "01890a5d-ac96-774b-bcce-b302099a80b2";

/// Run A parks; run B takes the hold, runs and parks again; only then
/// does A's report come back superseded. B's hold stays and A's late answer
/// destroys nothing, while B's own superseded answer still ends it.
#[tokio::test]
async fn test_a_late_superseded_answer_spares_the_next_runs_hold() {
    let capture = Capture::install();
    let engine = FakeEngine::default();
    let (holds, _clock) = holds(2);
    let first = holds.park(key(FLEET_A), id(RUN), sandbox(&engine).await);
    let first = bounded("the first run's park", first).await;
    assert!(matches!(first, crate::holds::Parked::Held(_)), "{first:?}");
    let taken = bounded("the next run's take", holds.take(&key(FLEET_A)))
        .await
        .unwrap();
    let next = id(NEXT_RUN);
    let parked = holds.park(key(FLEET_A), next, taken.sandbox);
    let parked = bounded("the next run's park", parked).await;
    assert!(
        matches!(parked, crate::holds::Parked::Held(_)),
        "{parked:?}"
    );

    holds.supersede(id(RUN));

    assert_eq!(fleets(&holds).await, [FLEET_A], "the next run's hold stays");
    let ended = releases(&capture);
    assert!(ended.is_empty(), "{ended:?}");
    holds.supersede(id(NEXT_RUN));
    bounded("the keeper's shutdown", holds.shutdown()).await;
    assert_eq!(releases(&capture), [released(FLEET_A, Release::Superseded)]);
    assert_eq!(engine.destroyed.load(Ordering::SeqCst), 1);
}
