//! When a lease neither takes the fleet's hold nor leaves one: the daemon did
//! not ask it to resume, the runner takes no new lease, or the sandbox died.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::borrow::Cow;

use afd_core::test_util::trace::Capture;
use afr_telemetry::labels::SandboxHold;

use super::revive_tests::holds_counted;
use super::tests::{
    NEXT_LEASE_ID, healthy, holding, holds_nothing, released, releases, resumed, settled,
};
use crate::holds::Release;
use crate::test_support::{
    Behaviour, FLEET_ID, FakeEngine, Freezer, LEASE_ID, OUTCOME, PROCESSED, Rig, lease, reported,
};

/// What the engine counts for a sandbox destroyed rather than held, as
/// (prepared, frozen, thawed, destroyed).
const DESTROYED_UNFROZEN: (usize, usize, usize, usize) = (1, 0, 0, 1);
/// A host the first lease's sandbox was not built to reach.
const CHANGED_HOST: &str = "api.github.com";

/// The daemon does not name this runner's hold the fleet's latest sandbox:
/// another runner ran the fleet since, or the event is a redelivery whose
/// first attempt the hold may carry. The lease builds fresh, and the stale
/// hold ends as superseded rather than serving it.
#[tokio::test(start_paused = true)]
async fn test_a_lease_not_resuming_builds_fresh_and_ends_the_hold() {
    let capture = Capture::install();
    let (rig, counted) = holding(healthy, FakeEngine::default(), Behaviour::Answer);

    rig.run(&lease(LEASE_ID, FLEET_ID, None)).await.unwrap();
    rig.run(&lease(NEXT_LEASE_ID, FLEET_ID, None))
        .await
        .unwrap();
    settled(&rig).await;

    assert_eq!(
        counted.read(),
        (2, 2, 0, 2),
        "built fresh, never thawed; the stale hold, then shutdown's"
    );
    assert_eq!(
        releases(&capture),
        [
            released(FLEET_ID, Release::Superseded),
            released(FLEET_ID, Release::Shutdown)
        ]
    );
}

/// The hold a lease does not resume is counted as superseded, never reused.
#[tokio::test(start_paused = true)]
async fn test_a_hold_not_resumed_is_counted_superseded() {
    let next = lease(NEXT_LEASE_ID, FLEET_ID, None);

    let counted = holds_counted(Freezer::Works, next).await;

    assert_eq!(
        counted,
        [
            SandboxHold::Parked,
            SandboxHold::Superseded,
            SandboxHold::Parked,
            SandboxHold::Shutdown
        ]
    );
}

/// The daemon says resume, and the lease takes the hold as before.
#[tokio::test(start_paused = true)]
async fn test_a_lease_resuming_takes_the_hold() {
    let (rig, counted) = holding(healthy, FakeEngine::default(), Behaviour::Answer);

    rig.run(&lease(LEASE_ID, FLEET_ID, None)).await.unwrap();
    rig.run(&resumed(NEXT_LEASE_ID, FLEET_ID)).await.unwrap();

    assert_eq!(counted.read(), (1, 2, 1, 0), "one sandbox, thawed and held");
}

/// The fleet's next lease is told to resume, but its policy reaches a host
/// the held sandbox was not built for: the hold ends as a mismatch, never
/// serving it, and the lease builds fresh.
#[tokio::test(start_paused = true)]
async fn test_a_lease_under_a_changed_policy_builds_fresh_and_ends_the_hold() {
    let capture = Capture::install();
    let (rig, counted) = holding(healthy, FakeEngine::default(), Behaviour::Answer);
    let mut next = resumed(NEXT_LEASE_ID, FLEET_ID);
    let reached = Cow::Borrowed(CHANGED_HOST);
    next.policy.network_policy.allow.push(reached);

    rig.run(&lease(LEASE_ID, FLEET_ID, None)).await.unwrap();
    rig.run(&next).await.unwrap();
    settled(&rig).await;

    assert_eq!(
        counted.read(),
        (2, 2, 0, 2),
        "built fresh, never thawed; the mismatched hold, then shutdown's"
    );
    assert_eq!(
        releases(&capture),
        [
            released(FLEET_ID, Release::Mismatch),
            released(FLEET_ID, Release::Shutdown)
        ]
    );
}

fn stop_leasing(rig: &Rig) {
    rig.lessee.halt.stop_leasing();
}

fn shut_down(rig: &Rig) {
    rig.shutdown.cancel();
}

/// A runner that takes no new lease holds nothing, since no lease could take
/// it: a lease that ends processed after leasing stopped, or while the runner
/// shuts down, destroys its sandbox and reports nothing held.
#[tokio::test(start_paused = true)]
async fn test_a_runner_taking_no_new_lease_holds_nothing() {
    for stop in [stop_leasing as fn(&Rig), shut_down] {
        let (mut rig, counted) = holding(healthy, FakeEngine::default(), Behaviour::Answer);
        stop(&rig);

        rig.run(&lease(LEASE_ID, FLEET_ID, None)).await.unwrap();

        let report = reported(&rig.calls());
        assert_eq!(report[OUTCOME], PROCESSED);
        holds_nothing(&report);
        assert_eq!(counted.read(), DESTROYED_UNFROZEN);
    }
}

/// A sandbox whose processes ended after its run is destroyed, never held:
/// there is nothing in it for the fleet's next lease to continue.
#[tokio::test(start_paused = true)]
async fn test_a_sandbox_no_longer_running_is_destroyed_not_held() {
    let engine = FakeEngine {
        freezer: Freezer::Dead,
        ..FakeEngine::default()
    };
    let (mut rig, counted) = holding(healthy, engine, Behaviour::Answer);

    rig.run(&lease(LEASE_ID, FLEET_ID, None)).await.unwrap();

    let report = reported(&rig.calls());
    assert_eq!(report[OUTCOME], PROCESSED);
    holds_nothing(&report);
    assert_eq!(counted.read(), DESTROYED_UNFROZEN);
}
