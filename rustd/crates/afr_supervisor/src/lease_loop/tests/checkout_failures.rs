//! How a lease's checkout falls short: refused before any sandbox is built,
//! failed inside the build with the run going on, or stopped between
//! repositories.
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::sync::atomic::Ordering;

use afd_core::test_util::trace::Capture;
use afr_executor::Unavailable;
use afr_tools::catalog::GIT;
use tokio_util::sync::CancellationToken;

use super::super::build::EVENT_BUILD_FAILED;
use super::tests::{REPOSITORY, WORKSPACE_ID, bound, is_empty, mints, rig, served};
use super::{
    DETAIL_CHECKOUT, EVENT_CHECKOUT_FAILED, EVENT_CHECKOUT_STOPPED, Ended, checkout_ended,
    each_until_stopped,
};
use crate::client::{Call, Verb};
use crate::test_support::{Answer, LEASE_ID, OUTCOME, PROCESSED, Rig, STARTUP_POSTURE, reported};

const FAILURE_REASON: &str = "failure_reason";
const FAILURE_DETAIL: &str = "failure_detail";

/// A lease whose checkout could never happen is refused before its first
/// model request, and no sandbox is ever built for it.
fn assert_refused_before_the_sandbox(rig: &Rig, calls: &[Call]) {
    let report = reported(calls);
    assert_eq!(report[FAILURE_REASON], STARTUP_POSTURE);
    assert_eq!(report[FAILURE_DETAIL], DETAIL_CHECKOUT);
    assert_eq!(rig.runs.load(Ordering::SeqCst), 0, "the model never ran");
    assert_eq!(
        rig.prepared.load(Ordering::SeqCst),
        0,
        "no sandbox was built"
    );
    assert_eq!(rig.destroyed.load(Ordering::SeqCst), 0);
}

/// A checkout that failed inside the build leaves the run going: the model
/// answers, its sandbox-side calls read `checkout`, the half-built sandbox is
/// destroyed, and the failure is logged once.
fn assert_ran_without_the_sandbox(rig: &mut Rig, calls: &[Call], capture: &Capture) {
    assert_eq!(reported(calls)[OUTCOME], PROCESSED);
    assert_eq!(rig.runs.load(Ordering::SeqCst), 1, "the run went on");
    assert_eq!(rig.lent(), [Some(Unavailable::Checkout)]);
    assert_eq!(
        rig.destroyed.load(Ordering::SeqCst),
        1,
        "the half-built sandbox is destroyed"
    );
    let failed = capture.only(EVENT_BUILD_FAILED);
    assert_eq!(failed.field("class"), Some(Unavailable::Checkout.as_str()));
    assert_eq!(failed.field("lease_id"), Some(LEASE_ID));
}

#[tokio::test]
async fn a_sandbox_the_host_cannot_reach_leaves_the_run_without_it() {
    let capture = Capture::install();
    let mut rig = rig(None, |_| None);
    served(&rig);

    rig.run(&bound(&[GIT.name()], WORKSPACE_ID)).await.unwrap();

    let calls = rig.calls();
    assert_ran_without_the_sandbox(&mut rig, &calls, &capture);
    assert_eq!(
        mints(&calls),
        0,
        "nothing is minted for a checkout that cannot happen"
    );
}

#[tokio::test]
async fn a_workspace_id_that_is_not_an_identifier_never_names_a_mirror() {
    let workspace = tempfile::tempdir().unwrap();
    let mut rig = rig(Some(workspace.path().to_owned()), |_| None);
    served(&rig);

    rig.run(&bound(&[GIT.name()], "../escape")).await.unwrap();

    let calls = rig.calls();
    assert_refused_before_the_sandbox(&rig, &calls);
    assert_eq!(mints(&calls), 0);
    assert!(is_empty(&rig.home.mirrors()));
}

#[tokio::test]
async fn a_refused_mint_leaves_the_run_without_the_sandbox() {
    let capture = Capture::install();
    let workspace = tempfile::tempdir().unwrap();
    let mut rig = rig(Some(workspace.path().to_owned()), |_| {
        Some(Answer::Fail(crate::error::refused(Verb::Mint, 403, None)))
    });
    served(&rig);

    rig.run(&bound(&[GIT.name()], WORKSPACE_ID)).await.unwrap();

    let calls = rig.calls();
    assert_ran_without_the_sandbox(&mut rig, &calls, &capture);
    assert!(is_empty(workspace.path()));
}

#[tokio::test]
async fn a_repository_that_will_not_check_out_leaves_the_run_without_it() {
    let capture = Capture::install();
    let workspace = tempfile::tempdir().unwrap();
    let mut rig = rig(Some(workspace.path().to_owned()), |_| None);
    let (tally, recorded) = afr_telemetry::testing::Tally::new();

    afr_telemetry::testing::scoped(tally, rig.run(&bound(&[GIT.name()], WORKSPACE_ID)))
        .await
        .unwrap();

    let calls = rig.calls();
    assert_ran_without_the_sandbox(&mut rig, &calls, &capture);
    let counted: Vec<_> = recorded
        .try_iter()
        .filter(|counted| matches!(counted, afr_telemetry::testing::Recorded::SandboxBuild(_)))
        .collect();
    assert!(
        matches!(
            counted.as_slice(),
            [afr_telemetry::testing::Recorded::SandboxBuild(
                afr_telemetry::labels::SandboxBuild::Checkout
            )]
        ),
        "{counted:?}"
    );
    let failed = capture.only(EVENT_CHECKOUT_FAILED);
    assert_eq!(failed.field("repository"), Some(REPOSITORY));
    assert_eq!(
        failed.field("error_code"),
        Some(afd_core::error_code::INTERNAL_OPERATION_FAILED.as_str())
    );
    assert!(
        failed
            .field("reason")
            .is_some_and(|reason| !reason.is_empty()),
        "an operator reads why the checkout failed"
    );
}

#[tokio::test]
async fn a_binding_that_does_not_parse_refuses_before_any_sandbox() {
    let workspace = tempfile::tempdir().unwrap();
    let mut rig = rig(Some(workspace.path().to_owned()), |_| None);
    let mut lease = bound(&[GIT.name()], WORKSPACE_ID);
    lease
        .policy
        .repository_binding
        .as_mut()
        .unwrap()
        .repositories = vec!["../escape".into()];

    rig.run(&lease).await.unwrap();

    let calls = rig.calls();
    assert_refused_before_the_sandbox(&rig, &calls);
    assert_eq!(mints(&calls), 0);
    assert!(is_empty(workspace.path()));
}

/// A stop raised while one repository is checked out ends the build before
/// the next one starts: the checkout that saw it finishes, and no later one
/// runs.
#[tokio::test]
async fn test_a_stop_between_repositories_starts_no_next_checkout() {
    let stop = CancellationToken::new();
    let mut started = Vec::new();

    let ended = each_until_stopped(["first", "second", "third"], &stop, |repository| {
        started.push(repository);
        stop.cancel();
        async { Ok(()) }
    })
    .await;

    assert!(matches!(ended, Err(Ended::Stopped)));
    assert_eq!(started, ["first"]);
}

/// What a checkout that did not finish logs: the stop's end is no failure,
/// so only a checkout that failed on its own warns.
#[test]
fn test_a_stopped_checkout_logs_no_failure() {
    let failure = crate::error::git(REPOSITORY, "fetched")("interrupted".into());
    let logged = |stopped| {
        let capture = Capture::install();
        checkout_ended(LEASE_ID, REPOSITORY, &failure, stopped);
        capture
            .events()
            .iter()
            .filter_map(|event| event.field("event").map(str::to_owned))
            .collect::<Vec<_>>()
    };

    assert_eq!(logged(true), [EVENT_CHECKOUT_STOPPED]);
    assert_eq!(logged(false), [EVENT_CHECKOUT_FAILED]);
}
