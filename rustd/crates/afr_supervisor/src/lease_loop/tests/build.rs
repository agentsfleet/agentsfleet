//! The build beside the turn: the first model request never waits for the
//! sandbox, a sandbox-side call does, and a failed build leaves the turn
//! going. How a build ends is in `build_endings.rs`; the checks that refuse a
//! lease before any model request are in `sandbox.rs`, `egress.rs` and
//! `checkout_failures.rs`.

use std::path::Path;
use std::sync::Arc;

use afd_core::test_util::trace::Capture;
use afd_wire::credentials::MintCredentialResponse;
use afd_wire::lease::LeasePayload;
use afd_wire::policy::{RepositoryAccess, RepositoryBinding};
use afr_executor::Unavailable;
use afr_tools::catalog::GIT;
use tokio::sync::mpsc;

use super::super::EVENT_COMPLETED;
use super::super::build::EVENT_BUILD_FAILED;
use super::*;
use crate::test_support::{FIXTURE_BRANCH, GRANTED_UNTIL, repository};

/// How long the stalled engine takes to prepare.
const PREPARE_TAKES: Duration = Duration::from_millis(500);
/// How long the token mint takes: past the first renewal, so a renewal the
/// daemon refuses lands while the checkout waits on it.
const MINT_TAKES: Duration = RENEWAL_TICK.saturating_mul(2);
pub(super) const HELD_UNTIL: &str = "held_until";
const REPOSITORY: &str = "acme/widget";
const WORKSPACE_ID: &str = "01890a5d-ac96-774b-bcce-b302099a805a";

pub(super) fn stalled() -> FakeEngine {
    FakeEngine {
        prepare_takes: PREPARE_TAKES,
        ..FakeEngine::default()
    }
}

/// An engine whose sandboxes offer `workspace` to the host for checkouts.
pub(super) fn checking_out(workspace: &Path) -> FakeEngine {
    FakeEngine {
        workspace: Some(workspace.to_owned()),
        ..FakeEngine::default()
    }
}

/// A lease offering `git`, bound to [`REPOSITORY`] at the fixture branch.
pub(super) fn bound() -> LeasePayload<'static> {
    let mut payload = lease(LEASE_ID, FLEET_ID);
    payload.event.workspace_id = WORKSPACE_ID.into();
    payload.policy.tools = vec![GIT.name().into()];
    payload.policy.repository_binding = Some(RepositoryBinding {
        repositories: vec![REPOSITORY.into()],
        access: RepositoryAccess::Write,
        base_branch: FIXTURE_BRANCH.into(),
    });
    payload
}

/// The rig's origin for [`REPOSITORY`], made with one commit.
pub(super) fn served(rig: &Rig) {
    repository(&rig.origins().join(format!("{REPOSITORY}.git")));
}

/// The daemon mints a token only after [`MINT_TAKES`].
fn late_mint(call: &Call) -> Option<Answer> {
    (call.verb == Verb::Mint).then(|| {
        let minted = MintCredentialResponse {
            token: "ghs_buildTokenNeverInTheWorkspace".into(),
            expires_at: GRANTED_UNTIL,
        };
        Answer::Late(MINT_TAKES, serde_json::to_vec(&minted).unwrap().into())
    })
}

/// [`late_mint`], and every renewal refused: the lease is cut mid-checkout.
fn late_mint_and_lost_renewal(call: &Call) -> Option<Answer> {
    late_mint(call).or_else(|| renewal_lost(call))
}

pub(super) fn build_failures(capture: &Capture) -> usize {
    capture
        .events()
        .iter()
        .filter(|event| event.field("event") == Some(EVENT_BUILD_FAILED))
        .count()
}

/// The first model request leaves while the sandbox is still being prepared,
/// a whole prepare before it is ready. The turn's sandbox call waits for the
/// build, then runs in the sandbox it made, and the line carries how long
/// the call was held up.
#[tokio::test(start_paused = true)]
async fn test_first_model_call_starts_before_the_sandbox_is_ready() {
    let capture = Capture::install();
    let (written, mut writes) = mpsc::unbounded_channel();
    let (ready, mut readied) = mpsc::unbounded_channel();
    let engine = FakeEngine {
        written: Some(written),
        ready: Some(ready),
        ..stalled()
    };
    let (began, mut started) = mpsc::unbounded_channel();
    let mut agent = FakeAgent::new(Behaviour::Answer);
    agent.began = Some(began);
    let mut rig = Rig::new(daemon(healthy), engine, agent);

    rig.run(&lease(LEASE_ID, FLEET_ID)).await.unwrap();

    let began = started.try_recv().unwrap();
    let ready = readied.try_recv().unwrap();
    assert_eq!(ready.duration_since(began), PREPARE_TAKES);
    assert_eq!(rig.lent(), [None], "the call got an executor");
    assert!(writes.try_recv().is_ok(), "the call wrote into the sandbox");
    let line = capture.only(EVENT_COMPLETED);
    assert_eq!(
        line.field("sandbox_wait_ms"),
        Some("500"),
        "{:?}",
        line.fields
    );
    assert_eq!(reported(&rig.calls())[OUTCOME], PROCESSED);
}

/// A build that fails while the turn runs leaves it going: the model answers,
/// its sandbox call reads `prepare`, nothing is held, and the failure is
/// logged once with its code.
#[tokio::test(start_paused = true)]
async fn test_build_failure_leaves_the_run_going() {
    let capture = Capture::install();
    let engine = FakeEngine {
        refuse: true,
        ..stalled()
    };
    let mut rig = rig(healthy, engine, Behaviour::Answer);

    rig.run(&lease(LEASE_ID, FLEET_ID)).await.unwrap();

    let report = reported(&rig.calls());
    assert_eq!(report[OUTCOME], PROCESSED, "{report}");
    assert!(
        report.get(HELD_UNTIL).is_none(),
        "nothing is held: {report}"
    );
    assert_eq!(rig.runs.load(Ordering::SeqCst), 1, "the run went on");
    assert_eq!(rig.lent(), [Some(Unavailable::Prepare)]);
    let failed = capture.only(EVENT_BUILD_FAILED);
    assert_eq!(failed.field("class"), Some(Unavailable::Prepare.as_str()));
    assert_eq!(failed.field("lease_id"), Some(LEASE_ID));
    assert_eq!(
        failed.field("error_code"),
        Some(afd_core::error_code::INTERNAL_OPERATION_FAILED.as_str()),
        "{:?}",
        failed.fields
    );
}

/// A renewal refused while the checkout waits on its token stops the build at
/// its next step: the partial sandbox is destroyed once, nothing is held,
/// nothing is checked out, and a stop logs no build failure.
#[tokio::test(start_paused = true)]
async fn test_partial_checkout_is_never_held() {
    let capture = Capture::install();
    let workspace = tempfile::tempdir().unwrap();
    let engine = checking_out(workspace.path());
    let frozen = Arc::clone(&engine.frozen);
    let mut rig = rig(late_mint_and_lost_renewal, engine, Behaviour::Stops);
    served(&rig);

    rig.run(&bound()).await.unwrap();

    let report = reported(&rig.calls());
    assert_eq!(report[FAILURE_REASON], RENEWAL_TERMINATE, "{report}");
    assert!(report.get(HELD_UNTIL).is_none(), "{report}");
    assert_eq!(rig.destroyed.load(Ordering::SeqCst), 1, "destroyed once");
    assert_eq!(frozen.load(Ordering::SeqCst), 0, "never frozen for a hold");
    assert!(
        std::fs::read_dir(workspace.path())
            .unwrap()
            .next()
            .is_none()
    );
    assert_eq!(rig.lent(), [Some(Unavailable::Stopped)]);
    assert_eq!(build_failures(&capture), 0, "a stop is not a failure");
}

/// An answer that arrives while the checkout is still running waits for it,
/// so the sandbox held for the fleet's next lease is a whole one.
#[tokio::test(start_paused = true)]
async fn test_an_answer_mid_checkout_holds_the_whole_sandbox() {
    let workspace = tempfile::tempdir().unwrap();
    let engine = checking_out(workspace.path());
    let frozen = Arc::clone(&engine.frozen);
    let mut rig = rig(late_mint, engine, Behaviour::Talks);
    rig.lessee.holds.resize(2);
    served(&rig);

    rig.run(&bound()).await.unwrap();

    let report = reported(&rig.calls());
    assert_eq!(report[OUTCOME], PROCESSED, "{report}");
    assert!(
        report.get(HELD_UNTIL).is_some(),
        "the sandbox is held: {report}"
    );
    assert_eq!(frozen.load(Ordering::SeqCst), 1);
    let readme = workspace.path().join("widget").join("README.md");
    assert!(readme.is_file(), "the checkout finished before the hold");
}
