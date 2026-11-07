//! The sandbox segment's three parts on the lease's last line: each times its
//! own step, a refused step still logs its part, and a step the lease never
//! reached is absent rather than zero.

#![expect(
    clippy::unwrap_used,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use afd_core::test_util::trace::{Capture, CapturedEvent};
use afd_wire::credentials::MintCredentialResponse;
use afd_wire::lease::LeasePayload;
use afd_wire::policy::{RepositoryAccess, RepositoryBinding};
use afd_wire::runner::NetworkPolicy;
use afr_tools::catalog::GIT;
use bytes::Bytes;

use super::super::{EVENT_COMPLETED, EVENT_FAILED};
use crate::client::{Call, Verb};
use crate::test_support::{
    Answer, Behaviour, FIXTURE_BRANCH, FLEET_ID, FakeAgent, FakeEngine, FakeResolver,
    GRANTED_UNTIL, LEASE_ID, Rig, Stalled, assigned, daemon, lease, repository,
};

const SANDBOX_MS: &str = "sandbox_ms";
const EGRESS_MS: &str = "egress_ms";
const PREPARE_MS: &str = "prepare_ms";
const CHECKOUT_MS: &str = "checkout_ms";
const MODEL_MS: &str = "model_ms";
/// The three parts, in the order the lease reaches them.
const PARTS: [&str; 3] = [EGRESS_MS, PREPARE_MS, CHECKOUT_MS];
/// A distinct time for each step, so a stopwatch around the wrong one shows.
const RESOLVE_TAKES: Duration = Duration::from_millis(10);
const PREPARE_TAKES: Duration = Duration::from_millis(30);
const MINT_TAKES: Duration = Duration::from_millis(50);
/// A registry host the resolver answers, and the address it answers with.
const REGISTRY: &str = "registry.example";
const REGISTRY_ADDRESS: Ipv4Addr = Ipv4Addr::new(192, 0, 2, 7);
const REPOSITORY: &str = "acme/widget";
const WORKSPACE_ID: &str = "01890a5d-ac96-774b-bcce-b302099a805a";
const TOKEN: &str = "ghs_partsTokenNeverInTheWorkspace";

fn millis(line: &CapturedEvent, field: &str) -> Option<u64> {
    line.field(field).map(|value| value.parse().unwrap())
}

fn ms(duration: Duration) -> Option<u64> {
    u64::try_from(duration.as_millis()).ok()
}

/// A lease offering `git`, bound to [`REPOSITORY`] at the fixture branch.
fn bound() -> LeasePayload<'static> {
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

/// A daemon that answers the token mint with `minted` only after
/// [`MINT_TAKES`], and everything else at once.
fn minting(minted: Bytes) -> impl Fn(&Call) -> Answer + Send + Sync + 'static {
    daemon(move |call| (call.verb == Verb::Mint).then(|| Answer::Late(MINT_TAKES, minted.clone())))
}

/// The mint's ordinary answer: [`TOKEN`], valid until the fixture's grant.
fn token() -> Bytes {
    let minted = MintCredentialResponse {
        token: TOKEN.into(),
        expires_at: GRANTED_UNTIL,
    };
    serde_json::to_vec(&minted).unwrap().into()
}

/// A resolver that answers [`REGISTRY`] after [`RESOLVE_TAKES`].
fn resolving() -> Stalled {
    Stalled {
        answers: FakeResolver::answering(&[(REGISTRY, &[IpAddr::V4(REGISTRY_ADDRESS)])]),
        takes: RESOLVE_TAKES,
    }
}

/// A lease that resolves, builds and checks out logs each step's time as its
/// part, and the three parts tile the sandbox segment.
#[tokio::test(start_paused = true)]
async fn test_lease_completed_splits_prepare_and_checkout() {
    let capture = Capture::install();
    let workspace = tempfile::tempdir().unwrap();
    let engine = FakeEngine {
        workspace: Some(workspace.path().to_owned()),
        prepare_takes: PREPARE_TAKES,
        ..FakeEngine::default()
    };
    let agent = FakeAgent::new(Behaviour::Answer);
    let rig = Rig::resolving(minting(token()), engine, agent, resolving());
    repository(&rig.origins().join(format!("{REPOSITORY}.git")));
    let egress = assigned(NetworkPolicy::AllowListEgress, &[REGISTRY]);

    rig.run_under(&bound(), &egress).await.unwrap();

    let line = capture.only(EVENT_COMPLETED);
    assert_eq!(
        millis(&line, EGRESS_MS),
        ms(RESOLVE_TAKES),
        "{:?}",
        line.fields
    );
    assert_eq!(
        millis(&line, PREPARE_MS),
        ms(PREPARE_TAKES),
        "{:?}",
        line.fields
    );
    assert_eq!(
        millis(&line, CHECKOUT_MS),
        ms(MINT_TAKES),
        "{:?}",
        line.fields
    );
    let parts: u64 = PARTS.iter().filter_map(|part| millis(&line, part)).sum();
    assert_eq!(
        millis(&line, SANDBOX_MS),
        Some(parts),
        "the parts tile the sandbox segment: {:?}",
        line.fields
    );
}

/// An egress that will not resolve still logs the time it took, and the
/// steps after it never started, so their parts are absent.
#[tokio::test(start_paused = true)]
async fn test_a_refused_egress_logs_only_its_part() {
    let capture = Capture::install();
    let unresolved = Stalled {
        answers: FakeResolver::default(),
        takes: RESOLVE_TAKES,
    };
    let agent = FakeAgent::new(Behaviour::Answer);
    let rig = Rig::resolving(
        daemon(|_call| None),
        FakeEngine::default(),
        agent,
        unresolved,
    );
    let egress = assigned(NetworkPolicy::AllowListEgress, &[REGISTRY]);

    rig.run_under(&lease(LEASE_ID, FLEET_ID), &egress)
        .await
        .unwrap();

    let line = capture.only(EVENT_FAILED);
    assert_eq!(
        millis(&line, EGRESS_MS),
        ms(RESOLVE_TAKES),
        "{:?}",
        line.fields
    );
    assert_eq!(
        millis(&line, SANDBOX_MS),
        ms(RESOLVE_TAKES),
        "{:?}",
        line.fields
    );
    for absent in [PREPARE_MS, CHECKOUT_MS, MODEL_MS] {
        assert_eq!(line.field(absent), None, "{absent}: {:?}", line.fields);
    }
}

/// A checkout refused after its token took [`MINT_TAKES`] logs that time as
/// its part, beside the prepare that came before it, and the turn ran on.
#[tokio::test(start_paused = true)]
async fn test_a_refused_checkout_still_logs_its_part() {
    let capture = Capture::install();
    let workspace = tempfile::tempdir().unwrap();
    let engine = FakeEngine {
        workspace: Some(workspace.path().to_owned()),
        prepare_takes: PREPARE_TAKES,
        ..FakeEngine::default()
    };
    let unreadable = Bytes::from_static(b"{}");
    let rig = Rig::new(
        minting(unreadable),
        engine,
        FakeAgent::new(Behaviour::Answer),
    );

    rig.run(&bound()).await.unwrap();

    let line = capture.only(EVENT_COMPLETED);
    assert_eq!(millis(&line, EGRESS_MS), Some(0), "{:?}", line.fields);
    assert_eq!(
        millis(&line, PREPARE_MS),
        ms(PREPARE_TAKES),
        "{:?}",
        line.fields
    );
    assert_eq!(
        millis(&line, CHECKOUT_MS),
        ms(MINT_TAKES),
        "{:?}",
        line.fields
    );
    assert!(line.field(MODEL_MS).is_some(), "{:?}", line.fields);
}
