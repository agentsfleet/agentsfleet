//! A superseded report ends only the hold its own lease parked.

#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on a fixture it cannot build"
)]

use std::sync::atomic::Ordering;

use af_core::test_util::trace::Capture;

use super::Release;
use super::tests::{FLEET_A, LEASE, fleets, holds, id, key, released, releases, sandbox};
use crate::test_support::{FakeEngine, bounded};

/// The fleet's next lease, which took the hold and parked it again.
const NEXT_LEASE: &str = "01890a5d-ac96-774b-bcce-b302099a80b2";

/// Lease A parks; lease B takes the hold, runs and parks again; only then
/// does A's report come back superseded. B's hold stays and A's late answer
/// destroys nothing, while B's own superseded answer still ends it.
#[tokio::test]
async fn test_a_late_superseded_answer_spares_the_next_leases_hold() {
    let capture = Capture::install();
    let engine = FakeEngine::default();
    let (holds, _clock) = holds(2);
    let first = holds.park(key(FLEET_A), id(LEASE), sandbox(&engine).await);
    let first = bounded("the first lease's park", first).await;
    assert!(matches!(first, crate::holds::Parked::Held(_)), "{first:?}");
    let taken = bounded("the next lease's take", holds.take(&key(FLEET_A)))
        .await
        .unwrap();
    let next = id(NEXT_LEASE);
    let parked = holds.park(key(FLEET_A), next, taken.sandbox);
    let parked = bounded("the next lease's park", parked).await;
    assert!(
        matches!(parked, crate::holds::Parked::Held(_)),
        "{parked:?}"
    );

    holds.supersede(id(LEASE));

    assert_eq!(
        fleets(&holds).await,
        [FLEET_A],
        "the next lease's hold stays"
    );
    let ended = releases(&capture);
    assert!(ended.is_empty(), "{ended:?}");
    holds.supersede(id(NEXT_LEASE));
    bounded("the keeper's shutdown", holds.shutdown()).await;
    assert_eq!(releases(&capture), [released(FLEET_A, Release::Superseded)]);
    assert_eq!(engine.destroyed.load(Ordering::SeqCst), 1);
}
