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

use super::{Assignment, Heartbeat};
use crate::client::{Call, Verb};
use crate::error;
use crate::test_support::{Answer, drain, json, plane};

fn probe() -> HostProbe {
    HostProbe {
        landlock: true,
        seccomp: true,
        cgroup_controllers: vec!["cpu".to_owned(), "memory".to_owned(), "pids".to_owned()],
        bubblewrap: true,
        kvm: Kvm::Absent,
        toolbox_filesystem: true,
    }
}

fn reply(status: &str, policy: bool, selftest: bool) -> Answer {
    let assigned = policy.then(|| {
        serde_json::json!({"sandbox_tier": "landlock_full", "network_policy": "allow_all",
            "registry_allowlist": [], "worker_count": 3, "extra_binds": []})
    });
    json(
        &serde_json::json!({"status": status, "assigned_policy": assigned, "degraded": false,
        "degraded_reason": null, "selftest_requested": selftest, "heartbeat_interval_ms": 2000}),
    )
}

fn sent(call: &Call) -> serde_json::Value {
    serde_json::from_slice(call.body.as_ref().unwrap()).unwrap()
}

#[tokio::test]
async fn a_self_test_waits_for_an_assignment_and_rides_the_next_beat() {
    let beats = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&beats);
    let (plane, mut calls) = plane(move |_call| match counted.fetch_add(1, Ordering::SeqCst) {
        0 => reply("ok", false, true),
        1 => reply("ok", true, true),
        _later => reply("drain", false, false),
    });
    let probe = probe();
    let mut heartbeat = Heartbeat::new(&plane, &probe);

    let unassigned = heartbeat.beat().await.unwrap();
    let assigned = heartbeat.beat().await.unwrap();
    let draining = heartbeat.beat().await.unwrap();

    assert_eq!(
        unassigned.workers.get(),
        1,
        "a runner with no assignment runs one"
    );
    assert_eq!(assigned.workers.get(), 3);
    assert_eq!(assigned.interval, Duration::from_millis(2000));
    assert_eq!(draining.status, HeartbeatStatus::Drain);
    assert_eq!(
        draining.workers.get(),
        3,
        "an absent policy keeps the last one"
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

#[test]
fn a_worker_takes_work_only_when_wanted() {
    let mut assignment = Assignment::initial();

    assert!(assignment.takes_work(0));
    assert!(!assignment.takes_work(1));
    assignment.status = HeartbeatStatus::Drain;
    assert!(!assignment.takes_work(0));
}

#[tokio::test(start_paused = true)]
async fn beating_publishes_each_assignment_and_a_stop_ends_the_runner() {
    let beats = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&beats);
    let (plane, _calls) = plane(move |_call| match counted.fetch_add(1, Ordering::SeqCst) {
        0 => Answer::Fail(error::unavailable(Verb::Heartbeat, 503)),
        1 => reply("ok", true, false),
        _later => reply("stop", false, false),
    });
    let probe = probe();
    let (published, mut watching) = watch::channel(Assignment::initial());
    let shutdown = CancellationToken::new();

    Heartbeat::new(&plane, &probe)
        .keep_beating(&published, &shutdown)
        .await;

    assert!(shutdown.is_cancelled());
    assert_eq!(
        beats.load(Ordering::SeqCst),
        3,
        "a failed beat keeps beating"
    );
    assert_eq!(watching.borrow_and_update().status, HeartbeatStatus::Stop);
}

#[tokio::test(start_paused = true)]
async fn a_refused_token_ends_the_runner_and_a_shutdown_ends_the_beat() {
    let (plane, _calls) = plane(|_call| Answer::Fail(error::refused(Verb::Heartbeat, 401, None)));
    let probe = probe();
    let (published, _watching) = watch::channel(Assignment::initial());
    let refused = CancellationToken::new();
    let stopped = CancellationToken::new();
    stopped.cancel();

    Heartbeat::new(&plane, &probe)
        .keep_beating(&published, &refused)
        .await;
    Heartbeat::new(&plane, &probe)
        .keep_beating(&published, &stopped)
        .await;

    assert!(refused.is_cancelled());
}
