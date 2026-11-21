//! A held sandbox revived for its fleet's next run: marked on the run's
//! span and logged when its executor answers, given up for a fresh sandbox
//! when it does not.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::sync::Arc;
use std::time::Duration;

use af_core::test_util::trace::Capture;
use af_observability::semconv::{ATTR_SANDBOX_REUSED, SPAN_RUNNER_RUN};
use af_wire::run::RunPayload;
use afr_telemetry::labels::SandboxHold;
use afr_telemetry::testing::{Recorded, Tally, scoped};

use super::tests::{NEXT_RUN_ID, healthy, holding, released, releases, resumed, settled};
use super::{DETAIL_SILENT, EVENT_REUSED, EVENT_THAW_FAILED};
use crate::client::{Call, Verb};
use crate::holds::{Holds, Release};
use crate::test_support::{
    Answer, Behaviour, EXECUTOR_GONE, FAILURE_REASON, FLEET_ID, FakeAgent, FakeEngine, Freezer,
    NO_FREEZER, OUTCOME, PROCESSED, RENEWAL_TERMINATE, RUN_ID, Rig, clock, daemon, payload,
    reported,
};

/// Long past the wait for a thawed executor's answer, on the paused clock: a
/// run still running then is stuck on a sandbox it should have given up.
const RUN_BOUND: Duration = Duration::from_secs(60);
/// The field a run's own log lines name it by.
const RUN_FIELD: &str = "run_id";
/// What the engine counts when a thawed hold is given up, as (prepared,
/// frozen, thawed, destroyed): thawed once, then built fresh and held in turn.
const THAWED_THEN_FRESH: (usize, usize, usize, usize) = (2, 2, 1, 2);

/// The held sandbox will not thaw: the hold is released as `thaw_failed`, the
/// log carries the kernel's reason, and the run runs in a fresh sandbox.
#[tokio::test(start_paused = true)]
async fn test_thaw_failure_falls_back_fresh() {
    let counted = gives_up_the_thawed_hold(Freezer::RefusesThaw, NO_FREEZER).await;

    assert_eq!(counted, (2, 2, 0, 2), "never thawed, then built fresh");
}

/// The executor a held sandbox thaws into died while it was frozen: it never
/// answers, so the hold is given up and the run runs in a fresh sandbox.
#[tokio::test(start_paused = true)]
async fn test_an_executor_silent_after_thaw_falls_back_fresh() {
    let counted = gives_up_the_thawed_hold(Freezer::ThawsSilent, DETAIL_SILENT).await;

    assert_eq!(counted, THAWED_THEN_FRESH);
}

/// The executor a held sandbox thaws into fails once it is back: the hold is
/// given up and the run runs in a fresh sandbox.
#[tokio::test(start_paused = true)]
async fn test_an_executor_failing_after_thaw_falls_back_fresh() {
    let counted = gives_up_the_thawed_hold(Freezer::ThawsBroken, EXECUTOR_GONE).await;

    assert_eq!(counted, THAWED_THEN_FRESH);
}

/// Runs a run that parks, then the fleet's next run over a sandbox that
/// thaws as `freezer` says: the hold is released as `thaw_failed`, the log
/// names the failure's code once and a reason carrying `cause`, and the run
/// runs fresh to `processed`. Answers what the engine counted.
#[expect(
    clippy::future_not_send,
    reason = "a thread-local trace `Capture` lives across `.await`; one thread polls it"
)]
async fn gives_up_the_thawed_hold(freezer: Freezer, cause: &str) -> (usize, usize, usize, usize) {
    let capture = Capture::install();
    let engine = FakeEngine {
        freezer,
        ..FakeEngine::default()
    };
    let (mut rig, counted) = holding(healthy, engine, Behaviour::Answer);

    rig.run(&payload(RUN_ID, FLEET_ID)).await.unwrap();
    let next = resumed(NEXT_RUN_ID, FLEET_ID);
    tokio::time::timeout(RUN_BOUND, rig.run(&next))
        .await
        .unwrap()
        .unwrap();
    settled(&rig).await;

    assert_eq!(reported(&rig.calls())[OUTCOME], PROCESSED);
    let failed = capture.only(EVENT_THAW_FAILED);
    let code = failed.field("error_code");
    let reason = failed.field("reason");
    assert_eq!(failed.field(RUN_FIELD), Some(NEXT_RUN_ID));
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
    let counted = holds_counted(Freezer::Works, resumed(NEXT_RUN_ID, FLEET_ID)).await;

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
    let counted = holds_counted(Freezer::RefusesThaw, resumed(NEXT_RUN_ID, FLEET_ID)).await;

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

/// Runs a run that parks, then `next`, over sandboxes that thaw as `freezer`
/// says, and reads back every hold the runner counted through to shutdown:
/// the runs' own counts and the task's.
pub(super) async fn holds_counted(freezer: Freezer, next: RunPayload<'static>) -> Vec<SandboxHold> {
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
        rig.run(&payload(RUN_ID, FLEET_ID)).await.unwrap();
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

/// The run that takes a hold says so: its span carries the reuse mark,
/// true where the first run's says false, and `sandbox_reused` names the
/// run, its fleet and how long the sandbox was held.
#[tokio::test(start_paused = true)]
async fn test_a_reused_sandbox_is_marked_on_the_run_span_and_logged() {
    let capture = Capture::install();
    let (rig, _counted) = holding(healthy, FakeEngine::default(), Behaviour::Answer);

    rig.run(&payload(RUN_ID, FLEET_ID)).await.unwrap();
    rig.run(&resumed(NEXT_RUN_ID, FLEET_ID)).await.unwrap();

    let marks: Vec<Option<String>> = capture
        .spans()
        .iter()
        .filter(|span| span.name == SPAN_RUNNER_RUN)
        .map(|span| span.field(ATTR_SANDBOX_REUSED).map(str::to_owned))
        .collect();
    assert_eq!(marks, [Some(false.to_string()), Some(true.to_string())]);
    let reused = capture.only(EVENT_REUSED);
    assert_eq!(reused.field(RUN_FIELD), Some(NEXT_RUN_ID));
    assert_eq!(reused.field("fleet_id"), Some(FLEET_ID));
    assert_eq!(
        reused.field("held_ms"),
        Some("0"),
        "the rig's clock stands still"
    );
}

/// Refuses every renewal of the fleet's next run, and of no other.
fn next_run_cut(call: &Call) -> Option<Answer> {
    (call.verb == Verb::Renew
        && renewal(call)
            .get("run_id")
            .and_then(serde_json::Value::as_str)
            == Some(NEXT_RUN_ID))
    .then(|| Answer::Fail(crate::error::refused(Verb::Renew, 409, None)))
}

/// The body a renewal sent, which every renewal carries.
fn renewal(call: &Call) -> serde_json::Value {
    serde_json::from_slice(call.body.as_ref().unwrap()).unwrap()
}

/// Holds up every run's memory a while, and refuses the renewals of the
/// fleet's next run: that run is cut before its build starts.
fn next_run_cut_while_its_memory_loads(call: &Call) -> Option<Answer> {
    match call.verb {
        Verb::Load => match daemon(|_call| None)(call) {
            Answer::Reply(bytes) => Some(Answer::Late(
                af_core::timing::RENEWAL_TICK.saturating_mul(2),
                bytes,
            )),
            other => Some(other),
        },
        _other => next_run_cut(call),
    }
}

/// The fleet's next run is cut while its memory loads, so its build never
/// starts: the hold it was told to resume is released as superseded at once,
/// not left in its slot until it lapses.
#[tokio::test(start_paused = true)]
async fn test_a_cut_before_the_build_releases_the_hold() {
    let capture = Capture::install();
    let (rig, counted) = holding(
        next_run_cut_while_its_memory_loads,
        FakeEngine::default(),
        Behaviour::Answer,
    );

    rig.run(&payload(RUN_ID, FLEET_ID)).await.unwrap();
    rig.run(&resumed(NEXT_RUN_ID, FLEET_ID)).await.unwrap();
    let before_shutdown = releases(&capture);
    settled(&rig).await;

    assert_eq!(
        before_shutdown,
        [released(FLEET_ID, Release::Superseded)],
        "released by the cut run, not by the shutdown"
    );
    assert_eq!(counted.read(), (1, 1, 0, 1), "held once, never thawed");
}

/// A renewal refused while the fleet's next run thaws its hold: the thaw
/// finishes before the build reads its stop, and the thawed sandbox is then
/// destroyed once and held by nobody.
#[tokio::test(start_paused = true)]
async fn test_cut_during_thaw_settles_the_hold() {
    let engine = FakeEngine {
        thaw_takes: af_core::timing::RENEWAL_TICK.saturating_mul(2),
        ..FakeEngine::default()
    };
    let (mut rig, counted) = holding(next_run_cut, engine, Behaviour::Answer);

    rig.run(&payload(RUN_ID, FLEET_ID)).await.unwrap();
    rig.run(&resumed(NEXT_RUN_ID, FLEET_ID)).await.unwrap();

    assert_eq!(
        counted.read(),
        (1, 1, 1, 1),
        "built and held once, thawed to the end, then destroyed once"
    );
    let report = reported(&rig.calls());
    assert_eq!(report[FAILURE_REASON], RENEWAL_TERMINATE, "{report}");
    assert!(report.get("held_until").is_none(), "{report}");
}
