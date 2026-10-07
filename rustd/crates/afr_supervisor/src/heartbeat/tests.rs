#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use afd_wire::runner::HeartbeatStatus;
use afr_sandbox::{HostProbe, Kvm};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use super::{Assignment, Heartbeat, MIN_HEARTBEAT_INTERVAL};
use crate::client::{Call, Verb};
use crate::error;
use crate::halt::Halt;
use crate::test_support::{Answer, INTERVAL_MS, drain, json, plane};

fn probe() -> HostProbe {
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
    json(
        &serde_json::json!({"status": status, "assigned_policy": assigned, "degraded": false,
        "degraded_reason": null, "selftest_requested": selftest, "heartbeat_interval_ms": interval_ms}),
    )
}

fn sent(call: &Call) -> serde_json::Value {
    serde_json::from_slice(call.body.as_ref().unwrap()).unwrap()
}

#[tokio::test]
async fn a_null_policy_fails_closed_and_a_self_test_waits_for_one() {
    let beats = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&beats);
    let (plane, mut calls) = plane(move |_call| match counted.fetch_add(1, Ordering::SeqCst) {
        0 => reply("ok", false, true, 2000),
        1 => reply("ok", true, true, 2000),
        _later => reply("drain", false, false, 2000),
    });
    let probe = probe();
    let mut heartbeat = Heartbeat::new(&plane, &probe);

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
    let (plane, _calls) = plane(|_call| reply("ok", true, false, 0));
    let probe = probe();

    let beat = Heartbeat::new(&plane, &probe).beat().await.unwrap();

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
        2 => reply("ok", true, false, INTERVAL_MS),
        _later => reply("stop", true, false, INTERVAL_MS),
    });
    let probe = probe();
    let (published, mut watching) = watch::channel(Assignment::initial());
    let halt = Halt::new(CancellationToken::new());
    let started = tokio::time::Instant::now();

    Heartbeat::new(&plane, &probe)
        .keep_beating(&published, &halt)
        .await;

    assert_eq!(
        beats.load(Ordering::SeqCst),
        4,
        "a failed first beat retries"
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

    Heartbeat::new(&plane, &probe)
        .keep_beating(&published, &refused)
        .await;
    Heartbeat::new(&plane, &probe)
        .keep_beating(&published, &Halt::new(stopped))
        .await;

    assert!(refused.token_refused() && refused.running().is_cancelled());
}
