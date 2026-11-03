//! The build beside the turn: the first model request never waits for the
//! sandbox, a sandbox-side call does, a failed build leaves the turn going,
//! and checks that would fail on every lease still refuse it before any model
//! request. How a build ends is in `build_endings.rs`.

use std::net::{IpAddr, Ipv4Addr};
use std::path::Path;
use std::sync::Arc;

use afd_core::test_util::trace::Capture;
use afd_wire::credentials::MintCredentialResponse;
use afd_wire::lease::{LeasePayload, SANDBOX_MEMORY_BYTES_MAX, SandboxLimits};
use afd_wire::policy::{RepositoryAccess, RepositoryBinding};
use afd_wire::runner::NetworkPolicy;
use afr_executor::Unavailable;
use afr_tools::catalog::{GIT, HTTP_REQUEST};
use tokio::sync::mpsc;

use super::super::EVENT_COMPLETED;
use super::super::build::EVENT_BUILD_FAILED;
use super::*;
use crate::test_support::{FIXTURE_BRANCH, FakeResolver, GRANTED_UNTIL, assigned, repository};

/// How long the stalled engine takes to prepare.
const PREPARE_TAKES: Duration = Duration::from_millis(500);
/// How long the token mint takes: past the first renewal, so a renewal the
/// daemon refuses lands while the checkout waits on it.
const MINT_TAKES: Duration = RENEWAL_TICK.saturating_mul(2);
pub(super) const HELD_UNTIL: &str = "held_until";
const REPOSITORY: &str = "acme/widget";
const WORKSPACE_ID: &str = "01890a5d-ac96-774b-bcce-b302099a805a";
/// A host a fleet may name that resolves to the cloud metadata service.
const INSIDE: &str = "inside.example";
const METADATA_ADDRESS: Ipv4Addr = Ipv4Addr::new(169, 254, 169, 254);

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
/// a whole prepare before it is ready, and the turn's sandbox call then gets
/// that sandbox's executor.
#[tokio::test(start_paused = true)]
async fn test_first_model_call_starts_before_the_sandbox_is_ready() {
    let engine = stalled();
    let ready = Arc::clone(&engine.ready);
    let agent = FakeAgent::new(Behaviour::Answer);
    let began = Arc::clone(&agent.began);
    let mut rig = Rig::new(daemon(healthy), engine, agent);

    rig.run(&lease(LEASE_ID, FLEET_ID)).await.unwrap();

    let began = began.lock().unwrap()[0];
    let ready = ready.lock().unwrap()[0];
    assert_eq!(ready.duration_since(began), PREPARE_TAKES);
    assert_eq!(
        *rig.lent.lock().unwrap(),
        [None],
        "the call got an executor"
    );
    assert_eq!(reported(&rig.calls())[OUTCOME], PROCESSED);
}

/// A sandbox call made while the build runs waits for it, then runs in the
/// sandbox the build made, and the line carries how long it was held up.
#[tokio::test(start_paused = true)]
async fn test_sandbox_tool_waits_for_the_build() {
    let capture = Capture::install();
    let (written, mut writes) = mpsc::unbounded_channel();
    let engine = FakeEngine {
        written: Some(written),
        ..stalled()
    };
    let mut rig = rig(healthy, engine, Behaviour::Answer);

    rig.run(&lease(LEASE_ID, FLEET_ID)).await.unwrap();

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
    assert_eq!(*rig.lent.lock().unwrap(), [Some(Unavailable::Prepare)]);
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

/// A size past the wire's bounds, or a host the fleet names at a private
/// address, refuses the lease before any model request, as it would refuse
/// every lease that names it.
#[tokio::test(start_paused = true)]
async fn test_static_refusals_precede_the_model_call() {
    let mut oversized = lease(LEASE_ID, FLEET_ID);
    oversized.limits = Some(SandboxLimits {
        cpu_millis: 4_000,
        memory_bytes: SANDBOX_MEMORY_BYTES_MAX + 1,
        disk_bytes: 20 << 30,
    });
    let mut inside = lease(LEASE_ID, FLEET_ID);
    inside.policy.network_policy.allow.push(INSIDE.into());
    let resolver = FakeResolver::answering(&[(INSIDE, &[IpAddr::V4(METADATA_ADDRESS)])]);
    let egress = assigned(NetworkPolicy::AllowListEgress, &[]);

    for refused in [&oversized, &inside] {
        let agent = FakeAgent::new(Behaviour::Answer);
        let mut rig = Rig::resolving(
            daemon(healthy),
            FakeEngine::default(),
            agent,
            resolver.clone(),
        );

        rig.run_under(refused, &egress).await.unwrap();

        assert_eq!(reported(&rig.calls())[FAILURE_REASON], STARTUP_POSTURE);
        assert_eq!(rig.runs.load(Ordering::SeqCst), 0, "no model request");
        assert_eq!(rig.prepared.load(Ordering::SeqCst), 0, "no sandbox built");
    }
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
    assert_eq!(*rig.lent.lock().unwrap(), [Some(Unavailable::Stopped)]);
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

/// A lease whose tools all run in the supervisor builds nothing, and its
/// turn's sandbox source answers that there is none.
#[tokio::test(start_paused = true)]
async fn test_supervisor_only_lease_starts_no_build() {
    let mut supervised = lease(LEASE_ID, FLEET_ID);
    supervised.policy.tools = vec![HTTP_REQUEST.name().into()];
    let mut rig = rig(healthy, FakeEngine::default(), Behaviour::Answer);

    rig.run(&supervised).await.unwrap();

    assert_eq!(rig.prepared.load(Ordering::SeqCst), 0);
    assert_eq!(*rig.lent.lock().unwrap(), [Some(Unavailable::NoSandbox)]);
    assert_eq!(reported(&rig.calls())[OUTCOME], PROCESSED);
}

/// A lease whose memory the daemon refuses ends before any build starts.
#[tokio::test(start_paused = true)]
async fn test_memory_refusal_starts_no_build() {
    let refused: Special = |call| {
        (call.verb == Verb::Hydrate).then(|| Answer::Fail(error::refused(Verb::Hydrate, 503, None)))
    };
    let mut rig = rig(refused, FakeEngine::default(), Behaviour::Answer);

    rig.run(&lease(LEASE_ID, FLEET_ID)).await.unwrap();

    assert_eq!(reported(&rig.calls())[FAILURE_REASON], STARTUP_POSTURE);
    assert_eq!(rig.prepared.load(Ordering::SeqCst), 0, "no build started");
    assert_eq!(rig.runs.load(Ordering::SeqCst), 0);
}
