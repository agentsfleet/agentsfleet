#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use afd_core::test_util::trace::Capture;
use afd_wire::credentials::MintCredentialResponse;
use afd_wire::lease::LeasePayload;
use afd_wire::policy::{RepositoryAccess, RepositoryBinding};
use afr_tools::catalog::{FILE_READ, GIT};
use afr_tools::sandbox::CREDENTIAL_GITHUB;

use super::{
    DETAIL_CHECKOUT, EVENT_CHECKOUT_COMPLETED, EVENT_CHECKOUT_FAILED, EVENT_CHECKOUT_STARTED,
};
use crate::client::{Call, Verb};
use crate::test_support::{
    Answer, Behaviour, FIRST_README, FIXTURE_BRANCH, FLEET_ID, FakeAgent, FakeEngine,
    GRANTED_UNTIL, LEASE_ID, OUTCOME, PROCESSED, Rig, STARTUP_POSTURE, daemon, git, lease,
    position, reported, repository,
};

const REPOSITORY: &str = "acme/widget";
const NAME: &str = "widget";
const WORKSPACE_ID: &str = "01890a5d-ac96-774b-bcce-b302099a805a";
const TOKEN: &str = "ghs_leaseTokenNeverInTheWorkspace";
const FAILURE_REASON: &str = "failure_reason";
const FAILURE_DETAIL: &str = "failure_detail";

/// A lease offering `tools`, bound to [`REPOSITORY`] at the fixture branch.
fn bound(tools: &[&str], workspace_id: &str) -> LeasePayload<'static> {
    let mut payload = lease(LEASE_ID, FLEET_ID, None);
    payload.event.workspace_id = workspace_id.to_owned().into();
    payload.policy.tools = tools.iter().map(|tool| (*tool).to_owned().into()).collect();
    payload.policy.repository_binding = Some(RepositoryBinding {
        repositories: vec![REPOSITORY.into()],
        access: RepositoryAccess::Write,
        base_branch: FIXTURE_BRANCH.into(),
    });
    payload
}

/// A rig whose daemon mints [`TOKEN`] unless `mint` answers first, and whose
/// sandboxes offer `workspace` to the host.
fn rig(workspace: Option<PathBuf>, mint: fn(&Call) -> Option<Answer>) -> Rig {
    let answers = move |call: &Call| match call.verb {
        Verb::Mint => Some(mint(call).unwrap_or_else(|| {
            crate::test_support::json(&MintCredentialResponse {
                token: TOKEN.into(),
                expires_at_ms: GRANTED_UNTIL,
            })
        })),
        _other => None,
    };
    let engine = FakeEngine {
        workspace,
        ..FakeEngine::default()
    };
    Rig::new(daemon(answers), engine, FakeAgent::new(Behaviour::Answer))
}

/// The rig's origin for [`REPOSITORY`], made with one commit.
fn served(rig: &Rig) -> PathBuf {
    let remote = rig.origins().join(format!("{REPOSITORY}.git"));
    repository(&remote);
    remote
}

fn mints(calls: &[Call]) -> usize {
    calls.iter().filter(|call| call.verb == Verb::Mint).count()
}

fn assert_refused(rig: &Rig, calls: &[Call]) {
    let report = reported(calls);
    assert_eq!(report[FAILURE_REASON], STARTUP_POSTURE);
    assert_eq!(report[FAILURE_DETAIL], DETAIL_CHECKOUT);
    assert_eq!(rig.runs.load(Ordering::SeqCst), 0, "no tool ever ran");
    assert_eq!(
        rig.destroyed.load(Ordering::SeqCst),
        1,
        "the sandbox is still destroyed"
    );
}

fn is_empty(dir: &Path) -> bool {
    fs::read_dir(dir).unwrap().next().is_none()
}

#[tokio::test]
async fn a_lease_offering_git_has_its_repository_checked_out_before_the_turn() {
    let capture = Capture::install();
    let workspace = tempfile::tempdir().unwrap();
    let mut rig = rig(Some(workspace.path().to_owned()), |_| None);
    let remote = served(&rig);

    rig.run(&bound(&[GIT.name()], WORKSPACE_ID)).await.unwrap();

    let copy = workspace.path().join(NAME);
    assert_eq!(
        fs::read_to_string(copy.join("README.md")).unwrap(),
        FIRST_README
    );
    assert_eq!(
        git(&copy, &["rev-parse", "HEAD"]),
        git(&remote, &["rev-parse", FIXTURE_BRANCH])
    );
    let calls = rig.calls();
    let mint = &calls[position(&calls, Verb::Mint).unwrap()];
    let asked: serde_json::Value = serde_json::from_slice(mint.body.as_ref().unwrap()).unwrap();
    assert_eq!(asked["integration"], CREDENTIAL_GITHUB);
    assert_eq!(mints(&calls), 1, "one token for the lease");
    assert_eq!(reported(&calls)[OUTCOME], PROCESSED);
    assert_eq!(rig.runs.load(Ordering::SeqCst), 1);
    let started = capture.only(EVENT_CHECKOUT_STARTED);
    assert_eq!(started.field("repository"), Some(REPOSITORY));
    let completed = capture.only(EVENT_CHECKOUT_COMPLETED);
    assert_eq!(completed.field("repository"), Some(REPOSITORY));
    assert_eq!(completed.field("fetched"), Some("cloned"));
    assert!(
        rig.home
            .mirrors()
            .join(WORKSPACE_ID)
            .join("acme")
            .join(format!("{NAME}.git"))
            .is_dir(),
        "the mirror is kept under the workspace's id"
    );
}

#[tokio::test]
async fn a_lease_without_a_process_tool_checks_nothing_out() {
    let workspace = tempfile::tempdir().unwrap();
    let mut rig = rig(Some(workspace.path().to_owned()), |_| None);
    served(&rig);

    rig.run(&bound(&[FILE_READ.name()], WORKSPACE_ID))
        .await
        .unwrap();

    let calls = rig.calls();
    assert_eq!(mints(&calls), 0);
    assert!(is_empty(workspace.path()));
    assert_eq!(reported(&calls)[OUTCOME], PROCESSED);
}

#[tokio::test]
async fn a_sandbox_the_host_cannot_reach_refuses_a_bound_lease() {
    let mut rig = rig(None, |_| None);
    served(&rig);

    rig.run(&bound(&[GIT.name()], WORKSPACE_ID)).await.unwrap();

    let calls = rig.calls();
    assert_refused(&rig, &calls);
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
    assert_refused(&rig, &calls);
    assert_eq!(mints(&calls), 0);
    assert!(is_empty(&rig.home.mirrors()));
}

#[tokio::test]
async fn a_refused_mint_fails_the_start() {
    let workspace = tempfile::tempdir().unwrap();
    let mut rig = rig(Some(workspace.path().to_owned()), |_| {
        Some(Answer::Fail(crate::error::refused(Verb::Mint, 403, None)))
    });
    served(&rig);

    rig.run(&bound(&[GIT.name()], WORKSPACE_ID)).await.unwrap();

    let calls = rig.calls();
    assert_refused(&rig, &calls);
    assert!(is_empty(workspace.path()));
}

#[tokio::test]
async fn a_repository_that_will_not_check_out_fails_the_start() {
    let capture = Capture::install();
    let workspace = tempfile::tempdir().unwrap();
    let mut rig = rig(Some(workspace.path().to_owned()), |_| None);

    rig.run(&bound(&[GIT.name()], WORKSPACE_ID)).await.unwrap();

    let calls = rig.calls();
    assert_refused(&rig, &calls);
    let failed = capture.only(EVENT_CHECKOUT_FAILED);
    assert_eq!(failed.field("repository"), Some(REPOSITORY));
    assert_eq!(
        failed.field("error_code"),
        Some(afd_core::error_code::INTERNAL_OPERATION_FAILED.as_str())
    );
}

#[tokio::test]
async fn a_binding_that_does_not_parse_refuses_before_any_mint() {
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
    assert_refused(&rig, &calls);
    assert_eq!(mints(&calls), 0);
    assert!(is_empty(workspace.path()));
}

#[tokio::test]
async fn every_bound_repository_is_checked_out_with_one_token() {
    const GADGET: &str = "acme/gadget";
    let workspace = tempfile::tempdir().unwrap();
    let mut rig = rig(Some(workspace.path().to_owned()), |_| None);
    served(&rig);
    repository(&rig.origins().join(format!("{GADGET}.git")));
    let mut lease = bound(&[GIT.name()], WORKSPACE_ID);
    lease
        .policy
        .repository_binding
        .as_mut()
        .unwrap()
        .repositories = vec![REPOSITORY.into(), GADGET.into()];

    rig.run(&lease).await.unwrap();

    for name in [NAME, "gadget"] {
        let readme = workspace.path().join(name).join("README.md");
        assert_eq!(fs::read_to_string(readme).unwrap(), FIRST_README, "{name}");
    }
    let calls = rig.calls();
    assert_eq!(mints(&calls), 1, "one token for every repository");
    assert_eq!(reported(&calls)[OUTCOME], PROCESSED);
}
