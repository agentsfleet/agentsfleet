//! A superseded report ends only the park its own run parked.

#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on a fixture it cannot build"
)]

use std::sync::atomic::Ordering;

use af_core::test_util::trace::Capture;

use super::Release;
use super::tests::{FLEET_A, RUN, fleets, id, key, parks, released, releases, sandbox};
use crate::test_support::{FakeEngine, bounded};

/// The fleet's next run, which took the park and parked it again.
const NEXT_RUN: &str = "01890a5d-ac96-774b-bcce-b302099a80b2";

/// Run A parks; run B takes the park, runs and parks again; only then
/// does A's report come back superseded. B's park stays and A's late answer
/// destroys nothing, while B's own superseded answer still ends it.
#[tokio::test]
async fn test_a_late_superseded_answer_spares_the_next_runs_park() {
    let capture = Capture::install();
    let engine = FakeEngine::default();
    let (parks, _clock) = parks(2);
    let first = parks.park(key(FLEET_A), id(RUN), sandbox(&engine).await);
    let first = bounded("the first run's park", first).await;
    assert!(
        matches!(first, crate::parks::Stowed::Parked(_)),
        "{first:?}"
    );
    let taken = bounded("the next run's take", parks.take(&key(FLEET_A)))
        .await
        .unwrap();
    let next = id(NEXT_RUN);
    let parked = parks.park(key(FLEET_A), next, taken.sandbox);
    let parked = bounded("the next run's park", parked).await;
    assert!(
        matches!(parked, crate::parks::Stowed::Parked(_)),
        "{parked:?}"
    );

    parks.supersede(id(RUN));

    assert_eq!(fleets(&parks).await, [FLEET_A], "the next run's park stays");
    let ended = releases(&capture);
    assert!(ended.is_empty(), "{ended:?}");
    parks.supersede(id(NEXT_RUN));
    bounded("the keeper's shutdown", parks.shutdown()).await;
    assert_eq!(releases(&capture), [released(FLEET_A, Release::Superseded)]);
    assert_eq!(engine.destroyed.load(Ordering::SeqCst), 1);
}
