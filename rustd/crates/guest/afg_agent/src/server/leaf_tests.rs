//! The tenant leaf as a save asks of it: which proof counts a tenant
//! process left behind, and the pin pause's freeze and thaw over the wire.

#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::borrow::Cow;
use std::fs;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use tokio::sync::mpsc;

use super::super::super::files::Workspace;
use super::super::super::launch::{Recorded, recorded_tenant};
use super::super::Session;
use crate::server::bind;
use crate::server::host_tests::Host;
use af_guest_wire::Sizes;
use af_guest_wire::protocol::{
    Leaf, LeafParams, METHOD_FREEZE, METHOD_LIST_DIR, METHOD_RECOVERY_IDLE, METHOD_THAW,
    NO_TENANT_LEAF_CODE, PathParams, RecoveryParams, RecoveryProof, request,
};

/// Longer than a local socket takes to connect.
const PATIENCE: Duration = Duration::from_secs(5);

/// What `session` answers `proof` with.
fn proved(
    session: &mut Session,
    answers: &mut mpsc::UnboundedReceiver<Bytes>,
    proof: RecoveryProof,
) -> bool {
    session.dispatch(&request(3, METHOD_RECOVERY_IDLE, &RecoveryParams { proof }).unwrap());
    let body: serde_json::Value = serde_json::from_slice(&answers.try_recv().unwrap()).unwrap();
    body.get("result")
        .and_then(serde_json::Value::as_bool)
        .unwrap()
}

/// A detached process a call left in the leaf keeps the file-jobs proof
/// busy, which a restore needs; the save's proof counts calls alone, since
/// its pause freezes that process instead.
#[tokio::test]
async fn only_the_file_jobs_proof_counts_a_process_left_in_the_leaf() {
    let (leaf, tenant) = recorded_tenant(Recorded::shared());
    let root = tempfile::tempdir().unwrap();
    let (outbound, mut answers) = mpsc::unbounded_channel();
    let (output, _events) = mpsc::channel(1);
    let mut session = Session::new(
        Arc::new(Workspace::open(root.path()).unwrap()),
        Arc::new(tenant),
        None,
        outbound,
        output,
        Sizes::DEFAULT,
    );
    let empty = proved(&mut session, &mut answers, RecoveryProof::FileJobs);
    fs::write(leaf.path().join("cgroup.procs"), "4242\n").unwrap();

    let file_jobs = proved(&mut session, &mut answers, RecoveryProof::FileJobs);
    let calls = proved(&mut session, &mut answers, RecoveryProof::Calls);

    assert!(empty, "an empty leaf and no call");
    assert!(!file_jobs, "a process left in the leaf");
    assert!(calls, "no call runs");
}

/// The host freezes the leaf and the agent, outside it, still lists the
/// workspace; the thaw follows. An agent with no leaf refuses both as holding
/// nothing, which the host tells from a freeze that failed.
#[tokio::test]
async fn the_host_freezes_and_thaws_the_leaf_while_the_agent_answers() {
    let freezer = Recorded::shared();
    let (_leaf, tenant) = recorded_tenant(Arc::clone(&freezer));
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("w");
    fs::create_dir(&root).unwrap();
    let tenant_leaf = LeafParams { leaf: Leaf::Tenant };
    let here = PathParams {
        path: Cow::Borrowed("."),
    };
    let mut answered = Vec::new();
    for (name, tenant) in [("tenant.sock", Some(tenant)), ("bare.sock", None)] {
        let socket = dir.path().join(name);
        let bound = bind(&socket).unwrap();
        let server = match tenant {
            Some(tenant) => bound.with_tenant(Arc::new(tenant)),
            None => bound,
        };
        let root = root.clone();
        let serving = tokio::spawn(async move { server.serve(&root).await });
        let mut host = Host::connect(&socket).await;
        let frozen = host.ask(METHOD_FREEZE, &tenant_leaf).await;
        let listed = host.ask(METHOD_LIST_DIR, &here).await;
        let thawed = host.ask(METHOD_THAW, &tenant_leaf).await;
        answered.push([frozen, listed, thawed]);
        drop(host);
        tokio::time::timeout(PATIENCE, serving)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }

    let code = |answer: &serde_json::Value| {
        answer
            .pointer("/error/code")
            .and_then(serde_json::Value::as_i64)
    };
    let [
        [frozen, listed, thawed],
        [bare_frozen, bare_listed, bare_thawed],
    ] = <[_; 2]>::try_from(answered).unwrap();
    assert!(frozen.get("result").is_some() && thawed.get("result").is_some());
    assert!(listed.get("result").is_some());
    assert_eq!(freezer.asked(), ["freeze", "thaw"]);
    let no_leaf = Some(i64::from(NO_TENANT_LEAF_CODE));
    assert_eq!((code(&bare_frozen), code(&bare_thawed)), (no_leaf, no_leaf));
    assert!(bare_listed.get("result").is_some());
}
