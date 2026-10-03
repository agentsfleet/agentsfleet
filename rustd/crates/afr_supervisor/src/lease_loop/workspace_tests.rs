#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::sync::atomic::Ordering;

use afd_core::bundle::BundleDigest;
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
