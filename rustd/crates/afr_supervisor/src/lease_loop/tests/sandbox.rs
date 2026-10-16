#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::sync::atomic::Ordering;

use afd_core::test_util::trace::Capture;
use afd_wire::lease::{SANDBOX_MEMORY_BYTES_MAX, SANDBOX_MEMORY_BYTES_MIN, SandboxLimits};
use afr_sandbox::Limits;
use tokio::sync::mpsc;

use crate::holds::Release;
use crate::test_support::{
    Behaviour, FAILURE_REASON, FLEET_ID, FakeAgent, FakeEngine, LEASE_ID, OUTCOME, PROCESSED, Rig,
    STARTUP_POSTURE, daemon, lease, reported,
};

/// Native sandbox work uses the same runner and daemon boundary.
fn rig(engine: FakeEngine) -> Rig {
    Rig::new(
        daemon(|_call| None),
        engine,
        FakeAgent::new(Behaviour::Answer),
    )
}

const REASON: &str = "reason";

/// One gibibyte.
const GIB: u64 = 1 << 30;

/// A size a lease might ask for: four cores, 8 GiB of memory, a 20 GiB disk.
const ASKED: SandboxLimits = SandboxLimits {
    cpu_millis: 4_000,
    memory_bytes: 8 * GIB,
    disk_bytes: 20 * GIB,
};

/// Runs `lease` on a rig whose engine reports the limits it was asked for.
async fn asked_for(limits: Option<SandboxLimits>) -> (Rig, Vec<Limits>) {
    let (asked, mut received) = mpsc::unbounded_channel();
    let rig = rig(FakeEngine {
        asked: Some(asked),
        ..FakeEngine::default()
    });
    let mut payload = lease(LEASE_ID, FLEET_ID);
    payload.limits = limits;
    rig.run(&payload).await.unwrap();
    let asked = std::iter::from_fn(|| received.try_recv().ok()).collect();
    (rig, asked)
}

#[tokio::test(start_paused = true)]
async fn a_sized_lease_builds_the_size_it_asked_for() {
    let (mut rig, asked) = asked_for(Some(ASKED)).await;

    let host = Limits::default();
    assert_eq!(
        asked,
        [Limits {
            cpu_millis: ASKED.cpu_millis,
            memory_bytes: ASKED.memory_bytes,
            disk_bytes: ASKED.disk_bytes,
            pids: host.pids,
        }],
        "the lease's size, with the host's process cap"
    );
    assert_eq!(reported(&rig.calls())[OUTCOME], PROCESSED);
}

#[tokio::test(start_paused = true)]
async fn a_lease_without_a_size_builds_the_hosts_defaults() {
    let (_rig, asked) = asked_for(None).await;

    assert_eq!(asked, [Limits::default()]);
}

#[tokio::test(start_paused = true)]
async fn a_size_past_its_bounds_refuses_the_lease_before_any_sandbox() {
    let past = SandboxLimits {
        memory_bytes: SANDBOX_MEMORY_BYTES_MIN - 1,
        ..ASKED
    };
    let capture = Capture::install();
    let (mut rig, asked) = asked_for(Some(past)).await;

    assert!(asked.is_empty(), "no sandbox is built for it");
    assert_eq!(rig.prepared.load(Ordering::SeqCst), 0);
    assert_eq!(rig.runs.load(Ordering::SeqCst), 0);
    assert_eq!(reported(&rig.calls())[FAILURE_REASON], STARTUP_POSTURE);
    let refused = capture.only("sandbox_size_refused");
    assert!(
        refused
            .field("error_code")
            .is_some_and(|code| !code.is_empty()),
        "the refusal names its code: {refused:?}"
    );
    assert!(
        refused.field("lease_id").is_some_and(|id| !id.is_empty()),
        "and its lease: {refused:?}"
    );
    assert!(
        refused
            .field(REASON)
            .is_some_and(|reason| !reason.is_empty() && !reason.starts_with('[')),
        "and why, its code only once: {refused:?}"
    );
}

#[test]
fn the_size_at_its_bound_is_built_and_one_past_is_refused() {
    let host = Limits::default();
    let at = SandboxLimits {
        memory_bytes: SANDBOX_MEMORY_BYTES_MAX,
        ..ASKED
    };
    let past = SandboxLimits {
        memory_bytes: SANDBOX_MEMORY_BYTES_MAX + 1,
        ..ASKED
    };

    assert_eq!(
        super::sized(Some(at), host).unwrap().memory_bytes,
        SANDBOX_MEMORY_BYTES_MAX
    );
    let refused = super::sized(Some(past), host).unwrap_err();
    assert_eq!(
        refused.code(),
        afd_core::error_code::INTERNAL_OPERATION_FAILED
    );
}

/// A held sandbox serves only a lease of its own size: the fleet's next lease
/// asking for another size is built fresh at that size, and the hold is
/// destroyed as `mismatch`.
#[tokio::test(start_paused = true)]
async fn test_a_lease_of_another_size_builds_fresh_and_ends_the_hold() {
    const NEXT_LEASE_ID: &str = "01890a5d-ac96-774b-bcce-b302099a805a";
    const RELEASED: &str = "sandbox_hold_released";
    let capture = Capture::install();
    let (asked, mut received) = mpsc::unbounded_channel();
    let mut rig = rig(FakeEngine {
        asked: Some(asked),
        ..FakeEngine::default()
    });
    rig.lessee.holds.resize(2);
    rig.run(&lease(LEASE_ID, FLEET_ID)).await.unwrap();
    let mut resized = lease(NEXT_LEASE_ID, FLEET_ID);
    resized.limits = Some(ASKED);
    resized.resume_hold = true;

    rig.run(&resized).await.unwrap();
    rig.lessee.holds.shutdown().await;

    let built: Vec<Limits> = std::iter::from_fn(|| received.try_recv().ok()).collect();
    assert_eq!(built.len(), 2, "the second lease built its own: {built:?}");
    assert_eq!(built[1].memory_bytes, ASKED.memory_bytes);
    assert_eq!(reported(&rig.calls())[OUTCOME], PROCESSED);
    let reasons: Vec<_> = capture
        .events()
        .iter()
        .filter(|event| event.field("event") == Some(RELEASED))
        .map(|event| event.field(REASON).unwrap().to_owned())
        .collect();
    let mismatch = Release::Mismatch.outcome().as_str();
    let shutdown = Release::Shutdown.outcome().as_str();
    assert_eq!(reasons, [mismatch, shutdown]);
    assert_eq!(
        rig.destroyed.load(Ordering::SeqCst),
        2,
        "the stale hold, then the fresh one at shutdown"
    );
}
