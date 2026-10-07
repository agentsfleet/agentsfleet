#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::sync::atomic::Ordering;

use afd_core::bundle::BundleDigest;
use afd_wire::lease::{SANDBOX_MEMORY_BYTES_MAX, SANDBOX_MEMORY_BYTES_MIN, SandboxLimits};
use afr_sandbox::Limits;
use bytes::Bytes;
use tokio::sync::mpsc;

use crate::client::Verb;
use crate::test_support::{
    Answer, Behaviour, FAILURE_REASON, FLEET_ID, FakeAgent, FakeEngine, LEASE_ID, OUTCOME,
    PROCESSED, RENEWAL_TERMINATE, Rig, STARTUP_POSTURE, Writes, daemon, lease, reported,
};

/// Why a failed landing must leave the turn unrun.
const NO_TURN: &str = "the model is never invoked";

/// The bundle's instructions.
const SKILL: &[u8] = b"skill";
/// A support file in a directory the workspace does not have yet.
const GUIDE_PATH: &str = "docs/guide.md";
const GUIDE: &[u8] = b"guide";

/// The canonical archive the importer stores, and the name it gave it.
fn bundle() -> (Bytes, String) {
    let mut builder = tar::Builder::new(Vec::new());
    for (path, content) in [("SKILL.md", SKILL), (GUIDE_PATH, GUIDE)] {
        let mut header = tar::Header::new_gnu();
        header.set_size(u64::try_from(content.len()).unwrap());
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, path, content).unwrap();
    }
    let mut digest = BundleDigest::new(SKILL, None);
    digest.support_file(GUIDE_PATH, GUIDE);
    (Bytes::from(builder.into_inner().unwrap()), digest.finish())
}

/// A rig whose daemon serves the bundle and whose sandboxes take `engine`.
fn rig(engine: FakeEngine) -> (Rig, String) {
    rig_ending_renewal(engine, false)
}

/// [`rig`], with a daemon that ends the lease at its first renewal when
/// `lose_lease` is set.
fn rig_ending_renewal(engine: FakeEngine, lose_lease: bool) -> (Rig, String) {
    let (archive, name) = bundle();
    let serves = move |call: &crate::client::Call| match call.verb {
        Verb::Bundle => Some(Answer::Reply(archive.clone())),
        Verb::Renew if lose_lease => Some(Answer::Fail(crate::error::refused(
            Verb::Renew,
            409,
            Some(afd_core::error_code::RUN_LEASE_LOST),
        ))),
        _other => None,
    };
    (
        Rig::new(daemon(serves), engine, FakeAgent::new(Behaviour::Answer)),
        name,
    )
}

#[tokio::test(start_paused = true)]
async fn a_bundles_support_files_land_in_the_workspace_before_the_turn() {
    let (written, mut landed) = mpsc::unbounded_channel();
    let (mut rig, name) = rig(FakeEngine {
        written: Some(written),
        ..FakeEngine::default()
    });

    rig.run(&lease(LEASE_ID, FLEET_ID, Some(&name)))
        .await
        .unwrap();

    let (path, content) = landed.try_recv().unwrap();
    assert_eq!((path.as_str(), content.as_ref()), (GUIDE_PATH, GUIDE));
    // The fake turn writes `a` itself: arriving after the support file is
    // what proves the files landed before the turn began.
    let (turns_own, _) = landed.try_recv().unwrap();
    assert_eq!(
        turns_own, "a",
        "only support files land first; SKILL.md is the prompt"
    );
    assert_eq!(reported(&rig.calls())[OUTCOME], PROCESSED);
}

#[tokio::test(start_paused = true)]
async fn support_files_that_will_not_land_fail_the_start_before_the_turn() {
    let (mut rig, name) = rig(FakeEngine {
        writes: Writes::Refuse,
        ..FakeEngine::default()
    });

    rig.run(&lease(LEASE_ID, FLEET_ID, Some(&name)))
        .await
        .unwrap();

    assert_eq!(reported(&rig.calls())[FAILURE_REASON], STARTUP_POSTURE);
    assert_eq!(rig.runs.load(Ordering::SeqCst), 0, "{NO_TURN}");
    assert_eq!(
        rig.destroyed.load(Ordering::SeqCst),
        1,
        "the sandbox is still destroyed"
    );
}

#[tokio::test(start_paused = true)]
async fn a_lease_that_ends_while_its_bundle_lands_stops_the_landing() {
    let (mut rig, name) = rig_ending_renewal(
        FakeEngine {
            writes: Writes::Stall,
            ..FakeEngine::default()
        },
        true,
    );

    rig.run(&lease(LEASE_ID, FLEET_ID, Some(&name)))
        .await
        .unwrap();

    assert_eq!(reported(&rig.calls())[FAILURE_REASON], RENEWAL_TERMINATE);
    assert_eq!(rig.runs.load(Ordering::SeqCst), 0);
    assert_eq!(
        rig.destroyed.load(Ordering::SeqCst),
        1,
        "the sandbox is freed at once"
    );
}

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
    let (rig, _name) = rig(FakeEngine {
        asked: Some(asked),
        ..FakeEngine::default()
    });
    let mut payload = lease(LEASE_ID, FLEET_ID, None);
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
    let (mut rig, asked) = asked_for(Some(past)).await;

    assert!(asked.is_empty(), "no sandbox is built for it");
    assert_eq!(rig.prepared.load(Ordering::SeqCst), 0);
    assert_eq!(rig.runs.load(Ordering::SeqCst), 0);
    assert_eq!(reported(&rig.calls())[FAILURE_REASON], STARTUP_POSTURE);
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
