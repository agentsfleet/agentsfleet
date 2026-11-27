//! The suite judged over fakes: a faithful engine passes every trial, on any
//! root, and each false claim fails the one trial that holds it, by name.

use std::sync::Arc;

use af_chunk_store::MemoryStore;

use self::fake::{FAR, Fake, Flaw, Landed, toolbox};
use super::{Context, TRIALS};
use crate::engine::Root;

mod fake;

/// The trials each flaw must fail.
const PAUSED: &str = "test_a_paused_sandbox_resumes_with_its_files_and_processes";
const MATRIX: &str = "test_the_verdict_matrix_refuses_every_unlisted_destination";
const SAVED_FILES: &str = "test_a_restored_snapshot_holds_the_saved_files";
const NO_PROCESS: &str = "test_no_process_survives_a_restore";
const CROSSES: &str = "test_a_snapshot_never_crosses_environments";
const OLDER_PLAN: &str = "test_a_restore_never_brings_back_an_older_plan";

/// Every trial `fake` fails, by name, with why. Every trial runs to a
/// verdict: a trial passes or fails, and has no third outcome.
#[expect(
    clippy::future_not_send,
    reason = "a trial runs on its caller's thread, as the lane runs it"
)]
async fn failures(fake: &Fake) -> Vec<(&'static str, String)> {
    let store = Arc::new(MemoryStore::default());
    let context = Context {
        far: FAR,
        checkout: &Landed,
        store: &store,
    };
    let mut failed = Vec::new();
    for trial in TRIALS {
        if let Err(why) = trial.run(fake, &context).await {
            failed.push((trial.name(), why.message().unwrap_or_default().to_owned()));
        }
    }
    failed
}

/// The names in `failed`.
fn names(failed: &[(&'static str, String)]) -> Vec<&'static str> {
    failed.iter().map(|(name, _why)| *name).collect()
}

/// A faithful engine passes on our toolbox and on the host's root alike; a
/// claimed `Pause` that answers `Unsupported` fails the pause trial alone,
/// saying so; a metadata address that answers fails the verdict matrix alone.
#[tokio::test]
async fn test_a_false_capability_fails_the_suite() {
    for root in [toolbox(), Root::Host] {
        let failed = failures(&Fake::faithful(root.clone())).await;
        assert_eq!(names(&failed), Vec::<&str>::new(), "{root:?}: {failed:?}");
    }

    let false_pause = failures(&Fake::flawed(Flaw::PauseUnsupported)).await;
    assert_eq!(names(&false_pause), [PAUSED], "{false_pause:?}");
    assert!(
        false_pause
            .iter()
            .all(|(_name, why)| why.contains("cannot park")),
        "{false_pause:?}"
    );

    let leaky = failures(&Fake::flawed(Flaw::MetadataAnswers)).await;
    assert_eq!(names(&leaky), [MATRIX], "{leaky:?}");
}

/// An engine that saves, restores, ends the old environment and returns equal
/// files passes every snapshot trial; one leaving an old process alive, or
/// one byte off, fails the trial that holds it, by name.
#[tokio::test]
async fn test_the_snapshot_trials_hold_every_engine_to_one_behaviour() {
    let cases = [
        (Flaw::Faithful, Vec::<&str>::new()),
        (Flaw::RestoreKeepsAProcess, vec![NO_PROCESS]),
        (Flaw::RestoreChangesAByte, vec![SAVED_FILES]),
    ];
    for (flaw, expected) in cases {
        let failed = failures(&Fake::snapshotting(flaw)).await;
        assert_eq!(names(&failed), expected, "{flaw:?}: {failed:?}");
    }

    let every: Vec<&str> = TRIALS
        .iter()
        .map(super::Trial::name)
        .filter(|name| name.contains("snapshot") || name.contains("restore"))
        .collect();
    let missing = failures(&Fake::snapshotting(Flaw::SnapshotsNotOffered)).await;
    assert_eq!(names(&missing), every, "{missing:?}");
    let undeclared = failures(&Fake::flawed(Flaw::SnapshotsNotDeclared)).await;
    assert_eq!(names(&undeclared), every, "{undeclared:?}");
    assert_eq!(every.len(), 6);
}

/// A snapshot never crosses environments: a faithful engine refuses a ref
/// naming another toolbox, guest agent or the host's root, naming
/// `environment`, before anything is read; one that restores it fails this
/// trial alone, by name.
#[tokio::test]
async fn test_a_snapshot_never_crosses_environments() {
    let faithful = failures(&Fake::snapshotting(Flaw::Faithful)).await;
    assert_eq!(names(&faithful), Vec::<&str>::new(), "{faithful:?}");

    let crossing = failures(&Fake::snapshotting(Flaw::RestoreCrossesEnvironments)).await;
    assert_eq!(names(&crossing), [CROSSES], "{crossing:?}");
    assert!(
        crossing
            .iter()
            .all(|(_name, why)| why.contains("environment")),
        "{crossing:?}"
    );
}

/// A restore never brings back an older plan: a snapshot taken under plan 3
/// and restored while 4 is in force leaves 4 in force; a fake whose restore
/// brings back 3 fails this trial alone, by name.
#[tokio::test]
async fn test_a_restore_never_brings_back_an_older_plan() {
    let rolled_back = failures(&Fake::snapshotting(Flaw::RestoreBringsBackThePlan)).await;
    assert_eq!(names(&rolled_back), [OLDER_PLAN], "{rolled_back:?}");
    assert!(
        rolled_back
            .iter()
            .all(|(_name, why)| why.contains("plan 4")),
        "{rolled_back:?}"
    );
}
