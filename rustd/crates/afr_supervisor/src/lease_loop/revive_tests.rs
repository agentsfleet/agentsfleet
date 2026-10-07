//! A held sandbox revived for its fleet's next lease: marked on the lease's
//! span and logged when its executor answers, given up for a fresh sandbox
//! when it does not.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::time::Duration;

use afd_core::test_util::trace::Capture;
use afd_observability::semconv::{ATTR_SANDBOX_REUSED, SPAN_RUNNER_LEASE};

use super::tests::{NEXT_LEASE_ID, healthy, holding, released, releases, settled};
use super::{DETAIL_SILENT, EVENT_REUSED, EVENT_THAW_FAILED};
use crate::holds::Release;
use crate::test_support::{
    Behaviour, EXECUTOR_GONE, FLEET_ID, FakeEngine, Freezer, LEASE_ID, OUTCOME, PROCESSED, lease,
    reported,
};

/// Long past the wait for a thawed executor's answer, on the paused clock: a
/// lease still running then is stuck on a sandbox it should have given up.
const LEASE_BOUND: Duration = Duration::from_secs(60);
/// The field a lease's own log lines name it by.
const LEASE_FIELD: &str = "lease_id";

/// The executor a held sandbox thaws into died while it was frozen: it never
/// answers, so the hold is given up and the lease runs in a fresh sandbox.
#[tokio::test(start_paused = true)]
async fn test_an_executor_silent_after_thaw_falls_back_fresh() {
    gives_up_the_thawed_hold(Freezer::ThawsSilent, DETAIL_SILENT).await;
}

/// The executor a held sandbox thaws into fails once it is back: the hold is
/// given up and the lease runs in a fresh sandbox.
#[tokio::test(start_paused = true)]
async fn test_an_executor_failing_after_thaw_falls_back_fresh() {
    gives_up_the_thawed_hold(Freezer::ThawsBroken, EXECUTOR_GONE).await;
}

/// Runs a lease that parks, then the fleet's next lease over a sandbox that
/// thaws as `freezer` says: the hold is released as `thaw_failed`, the log
/// names the failure's code and a reason carrying `cause`, and the lease runs
/// fresh to `processed`.
async fn gives_up_the_thawed_hold(freezer: Freezer, cause: &str) {
    let capture = Capture::install();
    let engine = FakeEngine {
        freezer,
        ..FakeEngine::default()
    };
    let (mut rig, counted) = holding(healthy, engine, Behaviour::Answer);

    rig.run(&lease(LEASE_ID, FLEET_ID, None)).await.unwrap();
    let next = lease(NEXT_LEASE_ID, FLEET_ID, None);
    tokio::time::timeout(LEASE_BOUND, rig.run(&next))
        .await
        .unwrap()
        .unwrap();
    settled(&rig).await;

    assert_eq!(reported(&rig.calls())[OUTCOME], PROCESSED);
    assert_eq!(
        counted.read(),
        (2, 2, 1, 2),
        "thawed once, given up, then built fresh and held in turn"
    );
    let failed = capture.only(EVENT_THAW_FAILED);
    let code = failed.field("error_code");
    let reason = failed.field("reason");
    assert_eq!(failed.field(LEASE_FIELD), Some(NEXT_LEASE_ID));
    assert!(code.is_some_and(|code| !code.is_empty()), "{failed:?}");
    assert!(
        reason.is_some_and(|reason| reason.contains(cause)),
        "the log names the cause: {failed:?}"
    );
    assert_eq!(
        releases(&capture),
        [
            released(FLEET_ID, Release::ThawFailed),
            released(FLEET_ID, Release::Shutdown)
        ]
    );
}

/// The lease that takes a hold says so: its span carries the reuse mark,
/// true where the first lease's says false, and `sandbox_reused` names the
/// lease, its fleet and how long the sandbox was held.
#[tokio::test(start_paused = true)]
async fn test_a_reused_sandbox_is_marked_on_the_lease_span_and_logged() {
    let capture = Capture::install();
    let (rig, _counted) = holding(healthy, FakeEngine::default(), Behaviour::Answer);

    rig.run(&lease(LEASE_ID, FLEET_ID, None)).await.unwrap();
    rig.run(&lease(NEXT_LEASE_ID, FLEET_ID, None))
        .await
        .unwrap();

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
