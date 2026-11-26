#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on a fixture it cannot build"
)]

use super::RecoveryDrillEngine;
use crate::UnsandboxedEngine;
use crate::engine::{
    Engine, Killed, Limits, Quiescence, SandboxSpec, SandboxState, fresh_sandbox_id,
};
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
    let (drilled, bare) = (fresh_sandbox_id(), fresh_sandbox_id());
    let killed = first
        .prepare(SandboxSpec::granted_by(&first, &drilled, Limits::DEFAULT).unwrap())
        .await
        .unwrap();
    let replacement = second
        .prepare(SandboxSpec::granted_by(&second, &drilled, Limits::DEFAULT).unwrap())
        .await
        .unwrap();
    let bare = unconfined
        .prepare(SandboxSpec::granted_by(&unconfined, &bare, Limits::DEFAULT).unwrap())
        .await
        .unwrap();

    let named = [
        killed.environment().root.toolbox().map(str::to_owned),
        replacement.environment().root.toolbox().map(str::to_owned),
        bare.environment().root.toolbox().map(str::to_owned),
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

/// A workspace checkpoint needs no tenant process left: a drill sandbox
/// reports itself quiet, its executor idle, and it serves on until destroy
/// removes it.
#[tokio::test]
async fn should_report_a_drill_sandbox_quiet() {
    let base = scratch();
    let id = fresh_sandbox_id();
    let engine = RecoveryDrillEngine::new(base.path().to_owned()).unwrap();
    let mut sandbox = engine
        .prepare(SandboxSpec::granted_by(&engine, &id, Limits::DEFAULT).unwrap())
        .await
        .unwrap();

    let quiescence = sandbox.quiesce().await.unwrap();
    let idle = sandbox.executor().recovery_idle().await.unwrap();
    let root = sandbox.workspace().unwrap().root.to_owned();
    let state = sandbox.state();
    sandbox.destroy().await.unwrap();

    assert_eq!(
        (quiescence, idle, state),
        (Quiescence::Quiet, true, SandboxState::Running)
    );
    assert_eq!(root, base.path().join(id.as_str()).join("workspace"));
    assert!(
        !base.path().join(id.as_str()).exists(),
        "the sandbox's directory is gone"
    );
}

/// The drill names a toolbox, but still shares the host's network and runs
/// under no cgroup: new addresses are refused as having no scope, and a kill
/// is never settled.
#[tokio::test]
async fn should_refuse_new_addresses_and_never_settle_a_kill() {
    let base = scratch();
    let id = fresh_sandbox_id();
    let engine = RecoveryDrillEngine::new(base.path().to_owned()).unwrap();
    let mut sandbox = engine
        .prepare(SandboxSpec::granted_by(&engine, &id, Limits::DEFAULT).unwrap())
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
