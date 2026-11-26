#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use af_core::clock::{FixedClock, SystemClock};
use af_core::id::Uuid7;
use af_wire::runner::Kvm;
use af_wire::runner::{HeartbeatStatus, NetworkPolicy};
use afr_engine::{Engine as _, HostProbe, Limits, ParkMode, SandboxSpec};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use super::{Assignment, Heartbeat};
use crate::client::{Call, Verb};
use crate::error;
use crate::halt::Halt;
use crate::holds::{BuiltUnder, HoldKey, Holds};
use crate::test_support::{self, Answer, FakeEngine, INTERVAL_MS, drain, json, plane, plane_every};

/// The fleet whose sandbox the runner holds.
pub(super) const FLEET: &str = "01890a5d-ac96-774b-bcce-b302099a80a1";
const RUN: &str = "01890a5d-ac96-774b-bcce-b302099a80b1";
/// A round long enough that a beat inside it came early.
pub(super) const TICK_MS: u32 = 3_600_000;
/// The event every release is logged under.
pub(super) const RELEASED: &str = "sandbox_hold_released";
/// The status of a daemon that wants the runner to keep going.
pub(super) const KEEP_GOING: &str = "ok";
/// The status of a daemon that wants the runner to stop.
pub(super) const STOP: &str = "stop";
/// The beat's field listing the fleets the runner holds.
pub(super) const HOLDS_FIELD: &str = "holds";

/// A registry holding nothing, as a runner that has run no run.
pub(super) fn idle_holds() -> Holds {
    Holds::start(Arc::new(SystemClock))
}

pub(super) fn probe() -> HostProbe {
    HostProbe {
        landlock: true,
        seccomp: true,
        cgroup_controllers: vec!["cpu".to_owned(), "memory".to_owned(), "pids".to_owned()],
        bubblewrap: true,
        kvm: Kvm::Absent,
        toolbox_filesystem: true,
        workspace_direct_io: None,
        egress: false,
    }
}

/// The assignment every answer here carries.
pub(super) fn policy() -> serde_json::Value {
    serde_json::json!({ "durable_tasks_enabled": true,"sandbox_tier": "landlock_full", "network_policy": "allow_all",
        "registry_allowlist": [], "worker_count": 3, "extra_binds": []})
}

pub(super) fn reply(status: &str, selftest: bool) -> Answer {
    answer(status, &policy(), selftest, &[])
}

/// An answer whose `release_holds` names `released`.
pub(super) fn releasing(status: &str, released: &[&str]) -> Answer {
    answer(status, &policy(), false, released)
}

/// The daemon's heartbeat answer, every field spelled once.
pub(super) fn answer(
    status: &str,
    assigned: &serde_json::Value,
    selftest: bool,
    released: &[&str],
) -> Answer {
    json(
        &serde_json::json!({"status": status, "assigned_policy": assigned, "degraded": false,
        "degraded_reason": null, "selftest_requested": selftest,
        "release_holds": released}),
    )
}

/// A registry over a stopped clock with room for `workers`, holding a sandbox
/// for [`FLEET`] that `engine` built.
pub(super) async fn holding(engine: &FakeEngine, workers: usize) -> Holds {
    let holds = Holds::start(Arc::new(FixedClock::at(jiff::Timestamp::UNIX_EPOCH)));
    holds.resize(workers);
    let parked = park(&holds, engine).await;
    assert!(
        matches!(parked, crate::holds::Stowed::Held(_)),
        "parked: {parked:?}"
    );
    holds
}

/// Parks a sandbox `engine` builds for [`FLEET`], and answers as `holds` did:
/// a sandbox it refuses is still the caller's to end.
pub(super) async fn park(holds: &Holds, engine: &FakeEngine) -> crate::holds::Stowed {
    let run = Uuid7::parse(RUN).unwrap();
    let spec = SandboxSpec::granted_by(engine, &run, Limits::default()).unwrap();
    let key = HoldKey {
        fleet: Uuid7::parse(FLEET).unwrap(),
        workspace: RUN.to_owned(),
        limits: Limits::default(),
        policy: BuiltUnder::allowing(&[]),
    };
    let sandbox = engine.prepare(spec).await.unwrap();
    let sandbox = sandbox.park(ParkMode::Pause).await.unwrap();
    let parked = holds.park(key, run, sandbox);
    test_support::bounded("the keeper's answer to a park", parked).await
}

pub(super) fn sent(call: &Call) -> serde_json::Value {
    serde_json::from_slice(call.body.as_ref().unwrap()).unwrap()
}

/// An answered beat carries its assignment to the runs, and the self-test it
/// asks for rides the next beat under that assignment's label. An answer whose
/// assignment is null is malformed, never an empty assignment.
#[tokio::test]
async fn an_assignment_reaches_the_runs_and_labels_the_self_test_it_asks_for() {
    let beats = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&beats);
    let (plane, mut calls) = plane(move |_call| match counted.fetch_add(1, Ordering::SeqCst) {
        0 => reply(KEEP_GOING, true),
        _later => answer("drain", &serde_json::Value::Null, false, &[]),
    });
    let probe = probe();
    let held = idle_holds();
    let mut heartbeat = Heartbeat::new(
        &plane,
        &probe,
        test_support::CAPABILITIES,
        &held,
        std::path::Path::new("."),
    );

    let assigned = heartbeat.beat().await.unwrap();
    let unassigned = heartbeat.beat().await;

    assert_eq!(assigned.workers, 3);
    assert_eq!(
        assigned.egress,
        test_support::assigned(NetworkPolicy::AllowAll, &[]),
        "the assigned posture reaches the runs"
    );
    assert!(
        unassigned.is_err(),
        "a null assignment is refused: {unassigned:?}"
    );
    let calls = drain(&mut calls);
    assert!(calls.iter().all(|call| call.verb == Verb::Heartbeat));
    assert!(calls.iter().all(|call| {
        sent(call)["capability_report"]["durable_tasks_v1"] == serde_json::Value::Bool(true)
    }));
    assert!(
        sent(&calls[0])["capability_report"]["landlock"]
            .as_bool()
            .unwrap()
    );
    assert!(
        sent(&calls[0])["selftest"].is_null(),
        "nothing has asked for a self-test yet"
    );
    assert_eq!(sent(&calls[1])["selftest"]["sandbox_tier"], "landlock_full");
}

#[test]
fn a_worker_takes_work_only_when_wanted() {
    let mut assignment = Assignment::initial();
    assert!(
        !assignment.takes_work(0),
        "nothing before the first answered beat"
    );

    assignment.workers = 1;
    assert!(assignment.takes_work(0));
    assert!(!assignment.takes_work(1));
    assignment.status = HeartbeatStatus::Drain;
    assert!(!assignment.takes_work(0));
}

#[tokio::test(start_paused = true)]
async fn a_failed_beat_retries_and_a_stop_ends_runs_in_flight() {
    let beats = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&beats);
    let every = Duration::from_millis(u64::from(INTERVAL_MS));
    let (plane, _calls) = plane_every(every, move |_call| {
        match counted.fetch_add(1, Ordering::SeqCst) {
            0 | 1 => Answer::Fail(error::unavailable(Verb::Heartbeat, 503)),
            2 => reply(KEEP_GOING, false),
            _later => reply(STOP, false),
        }
    });
    let probe = probe();
    let (published, mut watching) = watch::channel(Assignment::initial());
    let halt = Halt::new(CancellationToken::new());
    let started = tokio::time::Instant::now();

    let held = idle_holds();
    Heartbeat::new(
        &plane,
        &probe,
        test_support::CAPABILITIES,
        &held,
        std::path::Path::new("."),
    )
    .keep_beating(&published, &halt)
    .await;

    assert_eq!(
        beats.load(Ordering::SeqCst),
        5,
        "a failed first beat retries; the stop is followed by the last beat"
    );
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "retries back off from a short pause"
    );
    assert!(
        halt.running().is_cancelled(),
        "stop ends runs in flight too"
    );
    assert_eq!(watching.borrow_and_update().status, HeartbeatStatus::Stop);
}

#[tokio::test(start_paused = true)]
async fn a_refused_token_ends_the_runner_and_a_shutdown_ends_the_beat() {
    let (plane, _calls) = plane(|_call| Answer::Fail(error::refused(Verb::Heartbeat, 401, None)));
    let probe = probe();
    let (published, _watching) = watch::channel(Assignment::initial());
    let refused = Halt::new(CancellationToken::new());
    let stopped = CancellationToken::new();
    stopped.cancel();

    let held = idle_holds();
    Heartbeat::new(
        &plane,
        &probe,
        test_support::CAPABILITIES,
        &held,
        std::path::Path::new("."),
    )
    .keep_beating(&published, &refused)
    .await;
    let held = idle_holds();
    Heartbeat::new(
        &plane,
        &probe,
        test_support::CAPABILITIES,
        &held,
        std::path::Path::new("."),
    )
    .keep_beating(&published, &Halt::new(stopped))
    .await;

    assert!(refused.token_refused() && refused.running().is_cancelled());
}
