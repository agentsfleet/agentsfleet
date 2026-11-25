#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on a fixture it cannot build"
)]

use super::RecoveryDrillEngine;
use crate::UnsandboxedEngine;
use crate::engine::{Engine, Killed, Limits, SandboxRequest};
use crate::error::EgressRefusal;
use crate::network::Allowlist;

/// SHA-256 of the drill's toolbox label, spelled out.
const DRILL_TOOLBOX: &str = "661cc0d10c6445771f36bad2689af3353d581bb49b2940dcd816ee79dfdcd877";

fn scratch() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("afr")
        .tempdir_in("/tmp")
        .unwrap()
}

/// A replacement process restores only a checkpoint taken under its own
/// toolbox, so two engines that share no state must name the same one; the
/// unconfined engine beneath names none.
#[tokio::test]
async fn should_name_one_toolbox_from_every_drill_engine() {
    let (first_base, second_base) = (scratch(), scratch());
    let first = RecoveryDrillEngine::new(first_base.path().to_owned()).unwrap();
    let second = RecoveryDrillEngine::new(second_base.path().to_owned()).unwrap();
    let unconfined = UnsandboxedEngine::new(first_base.path().to_owned()).unwrap();
    let request = SandboxRequest::new("drilled", Limits::default());
    let killed = first.prepare(request).await.unwrap();
    let replacement = second.prepare(request).await.unwrap();
    let bare = unconfined
        .prepare(SandboxRequest::new("bare", Limits::default()))
        .await
        .unwrap();

    let named = [
        killed.toolbox_digest().map(str::to_owned),
        replacement.toolbox_digest().map(str::to_owned),
        bare.toolbox_digest().map(str::to_owned),
    ];
    for sandbox in [killed, replacement, bare] {
        sandbox.destroy().await.unwrap();
    }

    assert_eq!(
        named,
        [
            Some(DRILL_TOOLBOX.to_owned()),
            Some(DRILL_TOOLBOX.to_owned()),
            None
        ]
    );
}

/// A workspace checkpoint freezes, proves quiescence and thaws; each step
/// answers, the executor serves on, and destroy still removes the run.
#[tokio::test]
async fn should_freeze_prove_quiescence_and_thaw_a_drill_sandbox() {
    let base = scratch();
    let engine = RecoveryDrillEngine::new(base.path().to_owned()).unwrap();
    let mut sandbox = engine
        .prepare(SandboxRequest::new("frozen", Limits::default()))
        .await
        .unwrap();

    sandbox.freeze().await.unwrap();
    let quiescent = sandbox.recovery_quiescent().await.unwrap();
    sandbox.thaw().await.unwrap();
    let idle = sandbox.executor().recovery_idle().await.unwrap();
    let root = sandbox.workspace().unwrap().root.to_owned();
    let serving = sandbox.is_running();
    sandbox.destroy().await.unwrap();

    assert_eq!((quiescent, idle, serving), (true, true, true));
    assert_eq!(root, base.path().join("frozen").join("workspace"));
    assert!(
        !base.path().join("frozen").exists(),
        "the run's directory is gone"
    );
}

/// The run's name is checked by the unconfined engine beneath: a name that
/// is not one path segment builds no sandbox.
#[tokio::test]
async fn should_refuse_a_run_id_that_is_not_one_segment() {
    let base = scratch();
    let engine = RecoveryDrillEngine::new(base.path().to_owned()).unwrap();

    let refused = engine
        .prepare(SandboxRequest::new("nested/run", Limits::default()))
        .await
        .unwrap_err();

    assert_eq!(
        refused.code(),
        af_core::error_code::INTERNAL_OPERATION_FAILED
    );
    assert!(
        refused.to_string().contains("single path component"),
        "{refused}"
    );
    assert!(
        !base.path().join("nested").exists(),
        "nothing is made under the base"
    );
}

/// The drill names a toolbox and freezes, but still shares the host's
/// network and runs under no cgroup: new addresses are refused as having no
/// scope, and a kill is never settled.
#[tokio::test]
async fn should_refuse_new_addresses_and_never_settle_a_kill() {
    let base = scratch();
    let engine = RecoveryDrillEngine::new(base.path().to_owned()).unwrap();
    let mut sandbox = engine
        .prepare(SandboxRequest::new("unconfined", Limits::default()))
        .await
        .unwrap();

    let refused = sandbox.reallow(&Allowlist::new(Vec::new()).unwrap()).await;
    let killed = sandbox.kill().await;
    sandbox.destroy().await.unwrap();

    assert_eq!(
        refused.unwrap_err().egress_refusal(),
        Some(&EgressRefusal::NoScope)
    );
    assert!(matches!(killed, Killed::Unsettled(_)));
}
