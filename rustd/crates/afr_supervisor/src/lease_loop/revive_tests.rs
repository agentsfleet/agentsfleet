//! A held sandbox revived for its fleet's next lease: marked on the lease's
//! span and logged when its executor answers, given up for a fresh sandbox
//! when it does not.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::sync::Arc;
use std::time::Duration;

use afd_core::test_util::trace::Capture;
use afd_observability::semconv::{ATTR_SANDBOX_REUSED, SPAN_RUNNER_LEASE};
use afd_wire::lease::LeasePayload;
use afr_telemetry::labels::SandboxHold;
use afr_telemetry::testing::{Recorded, Tally, scoped};

use super::tests::{NEXT_LEASE_ID, healthy, holding, released, releases, resumed, settled};
use super::{DETAIL_SILENT, EVENT_REUSED, EVENT_THAW_FAILED};
use crate::holds::{Holds, Release};
use crate::test_support::{
    Behaviour, EXECUTOR_GONE, FLEET_ID, FakeAgent, FakeEngine, Freezer, LEASE_ID, NO_FREEZER,
    OUTCOME, PROCESSED, Rig, clock, daemon, lease, reported,
};

/// Long past the wait for a thawed executor's answer, on the paused clock: a
/// lease still running then is stuck on a sandbox it should have given up.
const LEASE_BOUND: Duration = Duration::from_secs(60);
/// The field a lease's own log lines name it by.
const LEASE_FIELD: &str = "lease_id";
/// What the engine counts when a thawed hold is given up, as (prepared,
/// frozen, thawed, destroyed): thawed once, then built fresh and held in turn.
const THAWED_THEN_FRESH: (usize, usize, usize, usize) = (2, 2, 1, 2);

/// The held sandbox will not thaw: the hold is released as `thaw_failed`, the
/// log carries the kernel's reason, and the lease runs in a fresh sandbox.
#[tokio::test(start_paused = true)]
async fn test_thaw_failure_falls_back_fresh() {
    let counted = gives_up_the_thawed_hold(Freezer::RefusesThaw, NO_FREEZER).await;

    assert_eq!(counted, (2, 2, 0, 2), "never thawed, then built fresh");
}

/// The executor a held sandbox thaws into died while it was frozen: it never
/// answers, so the hold is given up and the lease runs in a fresh sandbox.
#[tokio::test(start_paused = true)]
async fn test_an_executor_silent_after_thaw_falls_back_fresh() {
    let counted = gives_up_the_thawed_hold(Freezer::ThawsSilent, DETAIL_SILENT).await;

    assert_eq!(counted, THAWED_THEN_FRESH);
}

/// The executor a held sandbox thaws into fails once it is back: the hold is
/// given up and the lease runs in a fresh sandbox.
#[tokio::test(start_paused = true)]
async fn test_an_executor_failing_after_thaw_falls_back_fresh() {
    let counted = gives_up_the_thawed_hold(Freezer::ThawsBroken, EXECUTOR_GONE).await;

    assert_eq!(counted, THAWED_THEN_FRESH);
}

/// Runs a lease that parks, then the fleet's next lease over a sandbox that
/// thaws as `freezer` says: the hold is released as `thaw_failed`, the log
/// names the failure's code once and a reason carrying `cause`, and the lease
/// runs fresh to `processed`. Answers what the engine counted.
async fn gives_up_the_thawed_hold(freezer: Freezer, cause: &str) -> (usize, usize, usize, usize) {
    let capture = Capture::install();
    let engine = FakeEngine {
        freezer,
        ..FakeEngine::default()
    };
    let (mut rig, counted) = holding(healthy, engine, Behaviour::Answer);

    rig.run(&lease(LEASE_ID, FLEET_ID, None)).await.unwrap();
    let next = resumed(NEXT_LEASE_ID, FLEET_ID);
    tokio::time::timeout(LEASE_BOUND, rig.run(&next))
        .await
        .unwrap()
        .unwrap();
    settled(&rig).await;

    assert_eq!(reported(&rig.calls())[OUTCOME], PROCESSED);
    let failed = capture.only(EVENT_THAW_FAILED);
    let code = failed.field("error_code");
    let reason = failed.field("reason");
    assert_eq!(failed.field(LEASE_FIELD), Some(NEXT_LEASE_ID));
    assert!(code.is_some_and(|code| !code.is_empty()), "{failed:?}");
    assert!(
        reason.is_some_and(|reason| reason.contains(cause) && !reason.starts_with('[')),
        "the log names the cause, its code only once: {failed:?}"
    );
    assert_eq!(
        releases(&capture),
        [
            released(FLEET_ID, Release::ThawFailed),
            released(FLEET_ID, Release::Shutdown)
        ]
    );
    counted.read()
}

/// A park and the reuse that follows are each counted, reuse once the thawed
/// executor answered.
#[tokio::test(start_paused = true)]
async fn test_a_park_and_its_reuse_are_counted() {
    let counted = holds_counted(Freezer::Works, resumed(NEXT_LEASE_ID, FLEET_ID)).await;

    assert_eq!(
        counted,
        [
            SandboxHold::Parked,
            SandboxHold::Reused,
            SandboxHold::Parked,
            SandboxHold::Shutdown
        ]
    );
}

/// A hold that will not thaw was never reused: it counts as given up alone.
#[tokio::test(start_paused = true)]
async fn test_a_failed_thaw_counts_no_reuse() {
    let counted = holds_counted(Freezer::RefusesThaw, resumed(NEXT_LEASE_ID, FLEET_ID)).await;

    assert_eq!(
        counted,
        [
            SandboxHold::Parked,
            SandboxHold::ThawFailed,
            SandboxHold::Parked,
            SandboxHold::Shutdown
        ]
    );
}

/// Runs a lease that parks, then `next`, over sandboxes that thaw as `freezer`
/// says, and reads back every hold the runner counted through to shutdown:
/// the leases' own counts and the task's.
pub(super) async fn holds_counted(
    freezer: Freezer,
    next: LeasePayload<'static>,
) -> Vec<SandboxHold> {
    let (tally, recorded) = Tally::new();
    let engine = FakeEngine {
        freezer,
        ..FakeEngine::default()
    };
    let holds = Holds::recording(clock(), Arc::<Tally>::clone(&tally));
    holds.resize(2);
    let agent = FakeAgent::new(Behaviour::Answer);
    let rig = Rig::with_holds(daemon(healthy), engine, agent, holds);

    scoped(tally, async {
        rig.run(&lease(LEASE_ID, FLEET_ID, None)).await.unwrap();
        rig.run(&next).await.unwrap();
    })
    .await;
    settled(&rig).await;

    recorded
        .try_iter()
        .filter_map(|counted| match counted {
            Recorded::SandboxHold(hold) => Some(hold),
            _other => None,
        })
        .collect()
}

/// The lease that takes a hold says so: its span carries the reuse mark,
/// true where the first lease's says false, and `sandbox_reused` names the
/// lease, its fleet and how long the sandbox was held.
#[tokio::test(start_paused = true)]
async fn test_a_reused_sandbox_is_marked_on_the_lease_span_and_logged() {
    let capture = Capture::install();
    let (rig, _counted) = holding(healthy, FakeEngine::default(), Behaviour::Answer);

    rig.run(&lease(LEASE_ID, FLEET_ID, None)).await.unwrap();
    rig.run(&resumed(NEXT_LEASE_ID, FLEET_ID)).await.unwrap();

    let marks: Vec<Option<String>> = capture
        .spans()
        .iter()
        .filter(|span| span.name == SPAN_RUNNER_LEASE)
        .map(|span| span.field(ATTR_SANDBOX_REUSED).map(str::to_owned))
        .collect();
    assert_eq!(marks, [Some(false.to_string()), Some(true.to_string())]);
    let reused = capture.only(EVENT_REUSED);
    assert_eq!(reused.field(LEASE_FIELD), Some(NEXT_LEASE_ID));
    assert_eq!(reused.field("fleet_id"), Some(FLEET_ID));
    assert_eq!(
        reused.field("held_ms"),
        Some("0"),
        "the rig's clock stands still"
    );
}
