//! The tenant leaf as a save asks of it: which proof counts a tenant
//! process left behind, and the pin pause's freeze and thaw over the wire.

#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::fs;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use tokio::sync::mpsc;

use super::super::super::files::Workspace;
use super::super::super::launch::{Recorded, recorded_tenant};
use super::super::Session;
use crate::api::Executor as _;
use crate::protocol::{METHOD_RECOVERY_IDLE, RecoveryParams, RecoveryProof, request};
use crate::server::bind;
use crate::sizes::Sizes;

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

/// The host freezes the leaf and the executor, outside it, still lists the
/// workspace; the thaw follows. An executor with no leaf refuses both as
/// holding nothing, which the host tells from a freeze that failed.
#[tokio::test]
async fn the_host_freezes_and_thaws_the_leaf_while_the_executor_answers() {
    let freezer = Recorded::shared();
    let (_leaf, tenant) = recorded_tenant(Arc::clone(&freezer));
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("w");
    fs::create_dir(&root).unwrap();
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
        let client = crate::Client::connect_within(&socket, PATIENCE, Sizes::DEFAULT)
            .await
            .unwrap();
        let frozen = client.freeze_tenant().await;
        let listed = client.list_dir(".").await;
        let thawed = client.thaw_tenant().await;
        answered.push((frozen, listed.is_ok(), thawed));
        drop(client);
        serving.await.unwrap().unwrap();
    }

    let [
        (frozen, listed, thawed),
        (bare_frozen, bare_listed, bare_thawed),
    ] = <[_; 2]>::try_from(answered).unwrap();
    assert!(frozen.is_ok() && thawed.is_ok() && listed);
    assert_eq!(freezer.asked(), ["freeze", "thaw"]);
    assert!(bare_frozen.unwrap_err().is_no_tenant_leaf());
    assert!(bare_thawed.unwrap_err().is_no_tenant_leaf());
    assert!(bare_listed);
}
