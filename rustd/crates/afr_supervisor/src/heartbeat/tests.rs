#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use afd_core::clock::{FixedClock, SystemClock, UnixMillis};
use afd_core::error_code::INTERNAL_OPERATION_FAILED;
use afd_core::id::Uuid7;
use afd_core::test_util::trace::Capture;
use afd_wire::runner::HeartbeatStatus;
use afr_sandbox::{Engine as _, HostProbe, Kvm, Limits, SandboxRequest};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use super::{Assignment, EVENT_RELEASE_UNREADABLE, Heartbeat, MIN_HEARTBEAT_INTERVAL};
use crate::client::{Call, Verb};
use crate::error;
use crate::halt::Halt;
use crate::holds::{HoldKey, Holds, Release};
use crate::test_support::{Answer, FakeEngine, INTERVAL_MS, drain, json, plane};

/// The fleet whose sandbox the runner holds.
pub(super) const FLEET: &str = "01890a5d-ac96-774b-bcce-b302099a80a1";
/// A fleet busy on the runner's only worker.
const OTHER_FLEET: &str = "01890a5d-ac96-774b-bcce-b302099a80a2";
const LEASE: &str = "01890a5d-ac96-774b-bcce-b302099a80b1";
/// A name in `release_holds` that is no fleet id.
const NOT_AN_ID: &str = "not-a-fleet";
/// A tick long enough that a beat inside it came early.
pub(super) const TICK_MS: u32 = 3_600_000;
/// The event every release is logged under.
pub(super) const RELEASED: &str = "sandbox_hold_released";
/// The status of a daemon that wants the runner to keep going.
pub(super) const KEEP_GOING: &str = "ok";
/// The status of a daemon that wants the runner to stop.
pub(super) const STOP: &str = "stop";
/// The beat's field listing the fleets the runner holds.
pub(super) const HOLDS_FIELD: &str = "holds";

/// A registry holding nothing, as a runner that has run no lease.
fn idle_holds() -> Holds {
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
    }
}

fn reply(status: &str, policy: bool, selftest: bool, interval_ms: u32) -> Answer {
    let assigned = policy.then(|| {
        serde_json::json!({"sandbox_tier": "landlock_full", "network_policy": "allow_all",
            "registry_allowlist": [], "worker_count": 3, "extra_binds": []})
    });
    answer(
        status,
        &serde_json::json!(assigned),
        selftest,
        interval_ms,
        &[],
    )
}

/// An answer with no policy whose `release_holds` names `released`.
pub(super) fn releasing(status: &str, released: &[&str]) -> Answer {
    answer(status, &serde_json::Value::Null, false, TICK_MS, released)
}

/// The daemon's heartbeat answer, every field spelled once.
fn answer(
    status: &str,
    assigned: &serde_json::Value,
    selftest: bool,
    interval_ms: u32,
    released: &[&str],
) -> Answer {
    json(
        &serde_json::json!({"status": status, "assigned_policy": assigned, "degraded": false,
        "degraded_reason": null, "selftest_requested": selftest,
        "heartbeat_interval_ms": interval_ms, "release_holds": released}),
    )
}

/// A registry over a stopped clock with room for `workers`, holding a sandbox
/// for [`FLEET`] that `engine` built.
pub(super) async fn holding(engine: &FakeEngine, workers: usize) -> Holds {
    let holds = Holds::start(Arc::new(FixedClock::at(UnixMillis::from_millis(0))));
    holds.resize(workers);
    let request = SandboxRequest {
        lease_id: LEASE,
        limits: Limits::default(),
    };
    let key = HoldKey {
        fleet: Uuid7::parse(FLEET).unwrap(),
        workspace: LEASE.to_owned(),
        limits: Limits::default(),
        policy: String::new(),
    };
    let sandbox = engine.prepare(request).await.unwrap();
    let lease = Uuid7::parse(LEASE).unwrap();
    assert!(holds.park(key, lease, sandbox).await.is_some(), "parked");
    holds
}

pub(super) fn sent(call: &Call) -> serde_json::Value {
    serde_json::from_slice(call.body.as_ref().unwrap()).unwrap()
}

#[tokio::test]
async fn a_null_policy_fails_closed_and_a_self_test_waits_for_one() {
    let beats = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&beats);
    let (plane, mut calls) = plane(move |_call| match counted.fetch_add(1, Ordering::SeqCst) {
        0 => reply(KEEP_GOING, false, true, 2000),
        1 => reply(KEEP_GOING, true, true, 2000),
        _later => reply("drain", false, false, 2000),
    });
    let probe = probe();
    let held = idle_holds();
    let mut heartbeat = Heartbeat::new(&plane, &probe, &held);

    let unassigned = heartbeat.beat().await.unwrap();
    let assigned = heartbeat.beat().await.unwrap();
    let unreadable = heartbeat.beat().await.unwrap();

    assert_eq!(unassigned.workers, 0, "no policy, no work");
    assert_eq!(assigned.workers, 3);
    assert_eq!(assigned.interval, Duration::from_millis(2000));
    assert_eq!(unreadable.status, HeartbeatStatus::Drain);
    assert_eq!(
        unreadable.workers, 0,
        "a null policy is never the last one kept"
    );
    let calls = drain(&mut calls);
    assert!(calls.iter().all(|call| call.verb == Verb::Heartbeat));
    assert!(
        sent(&calls[0])["capability_report"]["landlock"]
            .as_bool()
            .unwrap()
    );
    assert!(
        sent(&calls[1])["selftest"].is_null(),
        "no assignment yet to label it with"
    );
    assert_eq!(sent(&calls[2])["selftest"]["sandbox_tier"], "landlock_full");
}

#[tokio::test]
async fn a_zero_interval_is_floored() {
    let (plane, _calls) = plane(|_call| reply(KEEP_GOING, true, false, 0));
    let probe = probe();

    let held = idle_holds();
    let beat = Heartbeat::new(&plane, &probe, &held).beat().await.unwrap();

    assert_eq!(beat.interval, MIN_HEARTBEAT_INTERVAL);
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
async fn a_failed_beat_retries_and_a_stop_ends_leases_in_flight() {
    let beats = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&beats);
    let (plane, _calls) = plane(move |_call| match counted.fetch_add(1, Ordering::SeqCst) {
        0 | 1 => Answer::Fail(error::unavailable(Verb::Heartbeat, 503)),
        2 => reply(KEEP_GOING, true, false, INTERVAL_MS),
        _later => reply(STOP, true, false, INTERVAL_MS),
    });
    let probe = probe();
    let (published, mut watching) = watch::channel(Assignment::initial());
    let halt = Halt::new(CancellationToken::new());
    let started = tokio::time::Instant::now();

    let held = idle_holds();
    Heartbeat::new(&plane, &probe, &held)
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
        "stop ends leases in flight too"
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
    Heartbeat::new(&plane, &probe, &held)
        .keep_beating(&published, &refused)
        .await;
    let held = idle_holds();
    Heartbeat::new(&plane, &probe, &held)
        .keep_beating(&published, &Halt::new(stopped))
        .await;

    assert!(refused.token_refused() && refused.running().is_cancelled());
}

/// The daemon names a held fleet inactive: its hold ends as `inactive` and its
/// sandbox is destroyed. A name that is no fleet id is logged with the code
/// for an unreadable identifier, and releases nothing.
#[tokio::test]
async fn test_a_released_hold_is_destroyed_as_inactive() {
    let capture = Capture::install();
    let engine = FakeEngine::default();
    let holds = holding(&engine, 2).await;
    let (plane, mut calls) = plane(|_call| releasing(KEEP_GOING, &[NOT_AN_ID, FLEET]));
    let probe = probe();

    Heartbeat::new(&plane, &probe, &holds).beat().await.unwrap();
    let left = holds.fleets().await;
    holds.shutdown().await;

    assert!(left.is_empty(), "{left:?}");
    assert_eq!(
        sent(&drain(&mut calls)[0])[HOLDS_FIELD],
        serde_json::json!([FLEET])
    );
    let released = capture.only(RELEASED);
    assert_eq!(released.field("fleet_id"), Some(FLEET));
    let inactive = Release::Inactive.outcome().as_str();
    assert_eq!(released.field("reason"), Some(inactive));
    assert_eq!(engine.destroyed.load(Ordering::SeqCst), 1);
    let unreadable = capture.only(EVENT_RELEASE_UNREADABLE);
    let code = INTERNAL_OPERATION_FAILED.as_str();
    assert_eq!(unreadable.field("error_code"), Some(code));
    assert_eq!(unreadable.field("fleet_id"), Some(NOT_AN_ID));
}

/// Holds released because the last free worker went to another fleet beat at
/// once, so the daemon stops routing that fleet here before the tick.
#[tokio::test(start_paused = true)]
async fn test_a_saturation_release_beats_at_once() {
    let engine = FakeEngine::default();
    let holds = holding(&engine, 1).await;
    let beats = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&beats);
    let (plane, mut calls) = plane(move |_call| match counted.fetch_add(1, Ordering::SeqCst) {
        0 => releasing(KEEP_GOING, &[]),
        _later => releasing(STOP, &[]),
    });
    let probe = probe();
    let (published, mut watching) = watch::channel(Assignment::initial());
    let halt = Halt::new(CancellationToken::new());
    let started = tokio::time::Instant::now();

    let beating = Heartbeat::new(&plane, &probe, &holds).keep_beating(&published, &halt);
    let saturating = async {
        watching.changed().await.unwrap();
        holds.occupy(Uuid7::parse(OTHER_FLEET).unwrap())
    };
    let (_beaten, _busy) = tokio::join!(beating, saturating);

    assert_eq!(
        beats.load(Ordering::SeqCst),
        3,
        "the saturation beat, then the last one"
    );
    assert!(
        started.elapsed() < Duration::from_millis(u64::from(TICK_MS)),
        "the second beat did not wait out the tick"
    );
    let calls = drain(&mut calls);
    assert_eq!(sent(&calls[0])[HOLDS_FIELD], serde_json::json!([FLEET]));
    assert_eq!(sent(&calls[1])[HOLDS_FIELD], serde_json::json!([]));
}
