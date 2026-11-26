//! The suite judged over fakes: a faithful engine passes every trial, on any
//! root, and each false claim fails the one trial that holds it, by name.

use self::fake::{FAR, Fake, Flaw, Landed, toolbox};
use super::{Context, TRIALS};
use crate::engine::Root;

mod fake;

/// The trials each flaw must fail.
const PAUSED: &str = "test_a_paused_sandbox_resumes_with_its_files_and_processes";
const MATRIX: &str = "test_the_verdict_matrix_refuses_every_unlisted_destination";
const SAVED_FILES: &str = "test_a_restored_snapshot_holds_the_saved_files";
const NO_PROCESS: &str = "test_no_process_survives_a_restore";

/// Every trial `fake` fails, by name, with why. Every trial runs to a
/// verdict: a trial passes or fails, and has no third outcome.
#[expect(
    clippy::future_not_send,
    reason = "a trial runs on its caller's thread, as the lane runs it"
)]
async fn failures(fake: &Fake) -> Vec<(&'static str, String)> {
    let context = Context {
        far: FAR,
        checkout: &Landed,
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
/// files passes all four snapshot trials; one leaving an old process alive,
/// or one byte off, fails the trial that holds it, by name.
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
}
