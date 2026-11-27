//! A parked sandbox revived for its fleet's next run: marked on the run's
//! span and logged when its guest agent answers, given up for a fresh sandbox
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
use afr_telemetry::labels::SandboxPark;
use afr_telemetry::testing::{Recorded, Tally, scoped};

use super::tests::{NEXT_RUN_ID, healthy, parking_rig, released, releases, resumed, settled};
use super::{DETAIL_SILENT, EVENT_RESUME_FAILED, EVENT_REUSED};
use crate::client::{Call, Verb};
use crate::parks::{Parks, Release};
use crate::test_support::{
    AGENT_GONE, Answer, Behaviour, FAILURE_REASON, FLEET_ID, FakeAgent, FakeEngine, NO_PARK,
    OUTCOME, PROCESSED, Parking, RENEWAL_TERMINATE, RUN_ID, Rig, clock, daemon, payload, reported,
};

/// Long past the wait for a resumed guest agent's answer, on the paused clock: a
/// run still running then is stuck on a sandbox it should have given up.
const RUN_BOUND: Duration = Duration::from_secs(60);
/// The field a run's own log lines name it by.
const RUN_FIELD: &str = "run_id";
/// What the engine counts when a resumed park is given up, as (prepared,
/// parked, resumed, destroyed): resumed once, then built fresh and parked in turn.
const RESUMED_THEN_FRESH: (usize, usize, usize, usize) = (2, 2, 1, 2);

/// The parked sandbox will not resume, and its engine destroys it: the park is
/// released as `thaw_failed`, the log carries the engine's reason, and the
/// run runs in a fresh sandbox.
#[tokio::test(start_paused = true)]
async fn test_resume_failure_falls_back_fresh() {
    let counted = gives_up_the_resumed_park(Parking::RefusesResume, NO_PARK).await;

    assert_eq!(
        counted,
        (2, 2, 0, 2),
        "never resumed, destroyed by its engine, then built fresh"
    );
}

/// The guest agent a parked sandbox resumes into died while it was parked: it
/// never answers, so the park is given up and the run runs in a fresh sandbox.
#[tokio::test(start_paused = true)]
async fn test_an_agent_silent_after_resume_falls_back_fresh() {
    let counted = gives_up_the_resumed_park(Parking::ResumesSilent, DETAIL_SILENT).await;

    assert_eq!(counted, RESUMED_THEN_FRESH);
}

/// The guest agent a parked sandbox resumes into fails once it is back: the park
/// is given up and the run runs in a fresh sandbox.
#[tokio::test(start_paused = true)]
async fn test_an_agent_failing_after_resume_falls_back_fresh() {
    let counted = gives_up_the_resumed_park(Parking::ResumesBroken, AGENT_GONE).await;

    assert_eq!(counted, RESUMED_THEN_FRESH);
}

/// Runs a run that parks, then the fleet's next run over a sandbox that
/// resumes as `parking` says: the park is released as `thaw_failed`, the log
/// names the failure's code once and a reason carrying `cause`, and the run
/// runs fresh to `processed`. Answers what the engine counted.
#[expect(
    clippy::future_not_send,
    reason = "a thread-local trace `Capture` lives across `.await`; one thread polls it"
)]
async fn gives_up_the_resumed_park(parking: Parking, cause: &str) -> (usize, usize, usize, usize) {
    let capture = Capture::install();
    let engine = FakeEngine {
        parking,
        ..FakeEngine::default()
    };
    let (mut rig, counted) = parking_rig(healthy, engine, Behaviour::Answer);

    rig.run(&payload(RUN_ID, FLEET_ID)).await.unwrap();
    let next = resumed(NEXT_RUN_ID, FLEET_ID);
    tokio::time::timeout(RUN_BOUND, rig.run(&next))
        .await
        .unwrap()
        .unwrap();
    settled(&rig).await;

    assert_eq!(reported(&rig.calls())[OUTCOME], PROCESSED);
    let failed = capture.only(EVENT_RESUME_FAILED);
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

/// A park and the reuse that follows are each counted, reuse once the resumed
/// guest agent answered.
#[tokio::test(start_paused = true)]
async fn test_a_park_and_its_reuse_are_counted() {
    let counted = parks_counted(Parking::Works, resumed(NEXT_RUN_ID, FLEET_ID)).await;

    assert_eq!(
        counted,
        [
            SandboxPark::Parked,
            SandboxPark::Reused,
            SandboxPark::Parked,
            SandboxPark::Shutdown
        ]
    );
}

/// A park that will not resume was never reused: it counts as given up alone.
#[tokio::test(start_paused = true)]
async fn test_a_failed_resume_counts_no_reuse() {
    let counted = parks_counted(Parking::RefusesResume, resumed(NEXT_RUN_ID, FLEET_ID)).await;

    assert_eq!(
        counted,
        [
            SandboxPark::Parked,
            SandboxPark::ThawFailed,
            SandboxPark::Parked,
            SandboxPark::Shutdown
        ]
    );
}

/// Runs a run that parks, then `next`, over sandboxes that resume as `parking`
/// says, and reads back every park the runner counted through to shutdown:
/// the runs' own counts and the task's.
pub(super) async fn parks_counted(parking: Parking, next: RunPayload<'static>) -> Vec<SandboxPark> {
    let (tally, recorded) = Tally::new();
    let engine = FakeEngine {
        parking,
        ..FakeEngine::default()
    };
    let parks = Parks::recording(clock(), Arc::<Tally>::clone(&tally));
    parks.resize(2);
    let agent = FakeAgent::new(Behaviour::Answer);
    let rig = Rig::with_parks(daemon(healthy), engine, agent, parks);

    scoped(tally, async {
        rig.run(&payload(RUN_ID, FLEET_ID)).await.unwrap();
        rig.run(&next).await.unwrap();
    })
    .await;
    settled(&rig).await;

    recorded
        .try_iter()
        .filter_map(|counted| match counted {
            Recorded::SandboxPark(park) => Some(park),
            _other => None,
        })
        .collect()
}

/// The run that takes a park says so: its span carries the reuse mark,
/// true where the first run's says false, and `sandbox_reused` names the
/// run, its fleet and how long the sandbox was parked.
#[tokio::test(start_paused = true)]
async fn test_a_reused_sandbox_is_marked_on_the_run_span_and_logged() {
    let capture = Capture::install();
    let (rig, _counted) = parking_rig(healthy, FakeEngine::default(), Behaviour::Answer);

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
        reused.field("parked_ms"),
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
                crate::test_support::ROUND.saturating_mul(2),
                bytes,
            )),
            other => Some(other),
        },
        _other => next_run_cut(call),
    }
}

/// The fleet's next run is cut while its memory loads, so its build never
/// starts: the park it was told to resume is released as superseded at once,
/// not left in its slot until it lapses.
#[tokio::test(start_paused = true)]
async fn test_a_cut_before_the_build_releases_the_park() {
    let capture = Capture::install();
    let (rig, counted) = parking_rig(
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
    assert_eq!(counted.read(), (1, 1, 0, 1), "parked once, never resumed");
}

/// A renewal refused while the fleet's next run resumes its park: the resume
/// finishes before the build reads its stop, and the resumed sandbox is then
/// destroyed once and parked by nobody.
#[tokio::test(start_paused = true)]
async fn test_cut_during_resume_settles_the_park() {
    let engine = FakeEngine {
        resume_takes: crate::test_support::ROUND.saturating_mul(2),
        ..FakeEngine::default()
    };
    let (mut rig, counted) = parking_rig(next_run_cut, engine, Behaviour::Answer);

    rig.run(&payload(RUN_ID, FLEET_ID)).await.unwrap();
    rig.run(&resumed(NEXT_RUN_ID, FLEET_ID)).await.unwrap();

    assert_eq!(
        counted.read(),
        (1, 1, 1, 1),
        "built and parked once, resumed to the end, then destroyed once"
    );
    let report = reported(&rig.calls());
    assert_eq!(report[FAILURE_REASON], RENEWAL_TERMINATE, "{report}");
    assert!(report.get("parked_until").is_none(), "{report}");
}
