//! How a run's build ends: counted by outcome, stopped before it starts,
//! failing after its turn answered, stopped by a failed turn, stopped
//! because it outlasted its answered turn, or not waited for at all on a
//! runner that takes no further run.

use std::sync::Arc;

use af_core::test_util::trace::Capture;
use af_wire::credentials::MintCredentialResponse;
use af_wire::run::RunPayload;
use afr_telemetry::labels::SandboxBuild;
use afr_telemetry::testing::{Recorded, Tally, scoped};

use super::super::workspace::{BUILD_AFTER_ANSWER, EVENT_BUILD_OUTLASTED};
use super::build::{HELD_UNTIL, bound, build_failures, checking_out, served, stalled};
use super::*;
use crate::test_support::GRANTED_UNTIL;

/// Holds up the run's memory past the first renewal, and refuses every
/// renewal: the run is cut before its build starts.
fn slow_load_and_lost_renewal(call: &Call) -> Option<Answer> {
    match call.verb {
        Verb::Load => match daemon(|_call| None)(call) {
            Answer::Reply(bytes) => Some(Answer::Late(RENEWAL_TICK.saturating_mul(2), bytes)),
            other => Some(other),
        },
        _other => renewal_lost(call),
    }
}

/// Runs `payload` on `rig` with a tally in scope; what builds it counted.
async fn builds_counted(rig: &Rig, payload: &RunPayload<'_>) -> Vec<SandboxBuild> {
    let (tally, recorded) = Tally::new();
    scoped(tally, rig.run(payload)).await.unwrap();
    recorded
        .try_iter()
        .filter_map(|counted| match counted {
            Recorded::SandboxBuild(build) => Some(build),
            _other => None,
        })
        .collect()
}

/// Every build is counted by how it ended: a failed build no longer ends its
/// run, so the count is where a host's fault shows. A checkout's failure
/// is counted in `checkout_failures.rs`.
#[tokio::test(start_paused = true)]
async fn test_every_build_is_counted_by_how_it_ended() {
    let refusing = FakeEngine {
        refuse: true,
        ..FakeEngine::default()
    };
    let cases = [
        (
            rig(healthy, FakeEngine::default(), Behaviour::Answer),
            SandboxBuild::Ready,
        ),
        (
            rig(healthy, refusing, Behaviour::Answer),
            SandboxBuild::Prepare,
        ),
        (
            rig(
                slow_load_and_lost_renewal,
                FakeEngine::default(),
                Behaviour::Stops,
            ),
            SandboxBuild::Stopped,
        ),
    ];

    for (rig, expected) in cases {
        let counted = builds_counted(&rig, &payload(RUN_ID, FLEET_ID)).await;

        assert_eq!(counted, [expected]);
    }
}

/// A run cut while its memory loads reaches its build already stopped,
/// and prepares nothing.
#[tokio::test(start_paused = true)]
async fn test_a_build_stopped_before_it_starts_prepares_nothing() {
    let mut rig = rig(
        slow_load_and_lost_renewal,
        FakeEngine::default(),
        Behaviour::Stops,
    );

    rig.run(&payload(RUN_ID, FLEET_ID)).await.unwrap();

    assert_eq!(reported(&rig.calls())[FAILURE_REASON], RENEWAL_TERMINATE);
    assert_eq!(rig.prepared.load(Ordering::SeqCst), 0, "nothing prepared");
}

/// A build that fails after the turn already answered keeps the turn's
/// ending in the report, holds nothing, and logs the failure.
#[tokio::test(start_paused = true)]
async fn test_late_build_failure_keeps_the_run_ending() {
    let capture = Capture::install();
    let engine = FakeEngine {
        refuse: true,
        ..stalled()
    };
    let mut rig = rig(healthy, engine, Behaviour::Talks);

    rig.run(&payload(RUN_ID, FLEET_ID)).await.unwrap();

    let report = reported(&rig.calls());
    assert_eq!(report[OUTCOME], PROCESSED, "{report}");
    assert!(report.get(HELD_UNTIL).is_none(), "{report}");
    assert_eq!(build_failures(&capture), 1);
}

/// A turn that fails while its build still runs stops the build: the sandbox
/// it finishes is destroyed once, nothing is held, and the report carries the
/// turn's own failure.
#[tokio::test(start_paused = true)]
async fn test_a_failed_turn_stops_its_build_and_holds_nothing() {
    let engine = stalled();
    let frozen = Arc::clone(&engine.frozen);
    let mut rig = rig(healthy, engine, Behaviour::Break);

    rig.run(&payload(RUN_ID, FLEET_ID)).await.unwrap();

    let report = reported(&rig.calls());
    assert_eq!(report[FAILURE_REASON], RUNNER_CRASH, "{report}");
    assert!(report.get(HELD_UNTIL).is_none(), "{report}");
    assert_eq!(rig.prepared.load(Ordering::SeqCst), 1);
    assert_eq!(rig.destroyed.load(Ordering::SeqCst), 1, "destroyed once");
    assert_eq!(frozen.load(Ordering::SeqCst), 0, "never held");
}

/// The daemon mints a token only long after the turn could wait for it.
fn stalled_mint(call: &Call) -> Option<Answer> {
    (call.verb == Verb::Mint).then(|| {
        let minted = MintCredentialResponse {
            token: "ghs_buildTokenNeverInTheWorkspace".into(),
            expires_at: GRANTED_UNTIL,
        };
        Answer::Late(
            BUILD_AFTER_ANSWER.saturating_mul(4),
            serde_json::to_vec(&minted).unwrap().into(),
        )
    })
}

/// A turn that answered waits a bounded time for its build: a checkout
/// stalled on its token past [`BUILD_AFTER_ANSWER`] is stopped, the report
/// goes processed with nothing held, and the stop is logged and counted.
#[tokio::test(start_paused = true)]
async fn test_a_build_that_outlasts_its_answer_is_stopped() {
    let capture = Capture::install();
    let workspace = tempfile::tempdir().unwrap();
    let mut rig = Rig::new(
        daemon(stalled_mint),
        checking_out(workspace.path()),
        FakeAgent::new(Behaviour::Talks),
    );
    served(&rig);
    let started = Instant::now();

    let counted = builds_counted(&rig, &bound()).await;

    let took = started.elapsed();
    let report = reported(&rig.calls());
    assert_eq!(report[OUTCOME], PROCESSED, "{report}");
    assert!(report.get(HELD_UNTIL).is_none(), "nothing held: {report}");
    assert_eq!(counted, [SandboxBuild::Stopped]);
    assert!(
        took < BUILD_AFTER_ANSWER.saturating_mul(2),
        "the report waited {took:?}, not the mint's whole stall"
    );
    assert_eq!(
        capture.only(EVENT_BUILD_OUTLASTED).field("run_id"),
        Some(RUN_ID)
    );
}

/// How far into an answered turn's build wait the runner stops running.
const STOPS_LEASING_AFTER: Duration = Duration::from_secs(1);

/// Leasing that stops while an answered turn waits for its build ends the
/// wait then: no run could take the hold any more, so the build is stopped
/// at that moment, not at the deadline.
#[tokio::test(start_paused = true)]
async fn test_leasing_that_stops_mid_wait_ends_the_wait() {
    let workspace = tempfile::tempdir().unwrap();
    let mut rig = Rig::new(
        daemon(stalled_mint),
        checking_out(workspace.path()),
        FakeAgent::new(Behaviour::Talks),
    );
    served(&rig);
    let lessee = Arc::clone(&rig.lessee);
    let stopping = tokio::spawn(async move {
        tokio::time::sleep(STOPS_LEASING_AFTER).await;
        lessee.halt.taking_runs().cancel();
    });
    let started = Instant::now();

    let counted = builds_counted(&rig, &bound()).await;

    let took = started.elapsed();
    stopping.await.unwrap();
    assert_eq!(reported(&rig.calls())[OUTCOME], PROCESSED);
    assert_eq!(counted, [SandboxBuild::Stopped]);
    assert!(
        took >= STOPS_LEASING_AFTER && took < BUILD_AFTER_ANSWER,
        "the report waited {took:?}"
    );
}

/// A runner that takes no further run holds no sandbox, so a turn that
/// answered waits for no build: the build is stopped at once, and nothing
/// is logged as outlasting the turn.
#[tokio::test(start_paused = true)]
async fn test_a_runner_that_stopped_leasing_waits_for_no_build() {
    let capture = Capture::install();
    let workspace = tempfile::tempdir().unwrap();
    let mut rig = Rig::new(
        daemon(stalled_mint),
        checking_out(workspace.path()),
        FakeAgent::new(Behaviour::Talks),
    );
    served(&rig);
    rig.lessee.halt.taking_runs().cancel();
    let started = Instant::now();

    let counted = builds_counted(&rig, &bound()).await;

    let took = started.elapsed();
    assert_eq!(reported(&rig.calls())[OUTCOME], PROCESSED);
    assert_eq!(counted, [SandboxBuild::Stopped]);
    assert!(took < BUILD_AFTER_ANSWER, "the report waited {took:?}");
    assert!(
        capture
            .events()
            .iter()
            .all(|event| event.field("event") != Some(EVENT_BUILD_OUTLASTED)),
        "a build nobody would hold is not waited for"
    );
}
