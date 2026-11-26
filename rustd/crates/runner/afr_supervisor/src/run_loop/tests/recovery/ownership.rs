//! A quiet sandbox and an API-only run share authenticated checkpoint ownership.

use super::*;
use crate::{
    client::{Call, Verb},
    egress::{Bound, Egress},
    holds::{BuiltUnder, HoldKey},
    run_loop::{Ids, segments::Timeline},
    test_support::{self, Answer, Behaviour, FakeAgent, FakeEngine, Parking, Rig},
};
use af_wire::{
    durable_task::{
        CheckpointAuthorizations, CheckpointPage, DurableTaskContext, DurableTaskInputsResponse,
    },
    run::RunPayload,
};
use afr_agent::Meter;
use afr_sandbox::Limits;
use std::sync::{Arc, OnceLock, atomic::Ordering};
use tokio_util::sync::CancellationToken;

const ID: &str = "019f0000-0000-7000-8000-000000000001";
const PAGE: &str = "019f0000-0000-7000-8000-000000000002";

#[path = "ownership/fixture.rs"]
mod fixture;
use fixture::{Fixture, saved};

fn history(state: Option<&DurableTaskRecovery>) -> Result<&DurableTaskRecovery> {
    state.ok_or_else(|| error::config("fixture expected authenticated recovery"))
}

#[tokio::test]
async fn api_recovery_uses_manifest_cursors_and_current_durable_task_identity() -> Result<()> {
    let fixture = Fixture::new(saved)?;
    let restored = fixture
        .run()?
        .recover_without_sandbox(Some(&fixture.prepared))
        .await?;
    let restored = history(restored.as_ref())?;
    assert_eq!(restored.task.durable_task_id, ID);
    assert_eq!(restored.task.generation, 7);
    assert_eq!(restored.task.covered_input_sequence, 3);
    assert_eq!(restored.task.confirmed_effect_watermark, 2);
    assert_eq!(restored.task.input_sequence, 7);
    assert_eq!(restored.history.as_slice(), b"");
    Ok(())
}

#[tokio::test]
async fn recovery_installs_history_only_into_a_quiet_sandbox() -> Result<()> {
    for parking in [Parking::Works, Parking::BusyTenant] {
        let fixture = Fixture::new(saved)?;
        let root = tempfile::tempdir()?;
        let (engine, sandbox) = sandbox(root.path(), parking).await?;
        let owner = OnceLock::new();
        let result = fixture
            .run()?
            .prepare_recovery(sandbox.as_ref(), &fixture.build(&owner))
            .await;
        assert_eq!(
            engine.parked.load(Ordering::SeqCst),
            0,
            "a restore parks nothing"
        );
        if parking == Parking::Works {
            result?;
            let state = owner
                .get()
                .ok_or_else(|| error::config("recovery owner missing"))?;
            assert_eq!(
                history(state.history.as_ref())?.task.covered_input_sequence,
                3
            );
            assert_eq!(
                history(state.history.as_ref())?
                    .task
                    .confirmed_effect_watermark,
                2
            );
            assert!(state.base.is_some());
            assert_eq!(state.base_paths.len(), 0);
        } else {
            let Err(failure) = result else {
                return Err(error::config("a busy sandbox was restored into"));
            };
            assert!(
                matches!(failure.kind(), error::ErrorKind::Config { detail }
                if *detail == "a tenant process still owns the recovery workspace"),
                "{failure:?}"
            );
            assert!(owner.get().is_none());
            assert_eq!(std::fs::read_dir(root.path())?.count(), 0);
        }
    }
    Ok(())
}

#[tokio::test]
async fn a_restore_refusal_installs_nothing_and_leaves_the_workspace_empty() -> Result<()> {
    let fixture = Fixture::new(|call| {
        (call.verb == Verb::DurableTaskCheckpointAuthorizations).then(|| {
            Answer::Fail(error::refused(
                call.verb,
                409,
                Some(af_core::error_code::DURABLE_TASK_STALE_STATE),
            ))
        })
    })?;
    let root = tempfile::tempdir()?;
    let (engine, sandbox) = sandbox(root.path(), Parking::Works).await?;
    let owner = OnceLock::new();
    let result = fixture
        .run()?
        .prepare_recovery(sandbox.as_ref(), &fixture.build(&owner))
        .await;
    let Err(failure) = result else {
        return Err(error::config("restore refusal was lost"));
    };
    assert_eq!(
        failure.code(),
        af_core::error_code::DURABLE_TASK_STALE_STATE
    );
    assert!(owner.get().is_none());
    assert_eq!(engine.parked.load(Ordering::SeqCst), 0);
    assert_eq!(std::fs::read_dir(root.path())?.count(), 0);
    Ok(())
}

#[tokio::test]
async fn repository_recovery_without_a_sandbox_has_no_native_read() -> Result<()> {
    let mut fixture = Fixture::new(|_| None)?;
    let task = fixture
        .prepared
        .inputs
        .current_durable_task
        .as_mut()
        .ok_or_else(|| error::config("fixture task missing"))?;
    task.target = DurableTaskTarget::ExistingPr {
        repository: "acme/widget".to_owned(),
        repository_id: 42,
        number: 3,
        branch: "repair".to_owned(),
        base: "main".to_owned(),
        head: "a".repeat(40),
        tree: "b".repeat(40),
        url: "https://github.com/acme/widget/pull/3".to_owned(),
        workflows: vec![],
    };
    fixture.run.durable_task = fixture.prepared.inputs.current_durable_task.clone();
    let result = fixture
        .run()?
        .recover_without_sandbox(Some(&fixture.prepared))
        .await;
    let Err(failure) = result else {
        return Err(error::config("unowned repository recovery accepted"));
    };
    assert!(matches!(failure.kind(), error::ErrorKind::Config { detail }
        if *detail == "repository recovery requires a sandbox"));
    assert_eq!(fixture.rig.calls().len(), 0);
    Ok(())
}

#[path = "ownership/repository.rs"]
mod repository_tests;

#[path = "ownership/requirements.rs"]
mod requirement_tests;

#[path = "ownership/boundary.rs"]
mod boundary_tests;

#[path = "ownership/turn.rs"]
mod turn_tests;

#[path = "ownership/final_capture.rs"]
mod final_capture_tests;

#[path = "ownership/checkout.rs"]
mod checkout_tests;

#[path = "ownership/hold.rs"]
mod hold_tests;

async fn sandbox(
    root: &std::path::Path,
    parking: Parking,
) -> Result<(FakeEngine, Box<dyn Sandbox>)> {
    let engine = FakeEngine {
        workspace: Some(root.to_owned()),
        toolbox: Some(api_toolbox()),
        parking,
        ..FakeEngine::default()
    };
    let sandbox = engine
        .sandbox()
        .await
        .map_err(|source| error::boundary("fixture sandbox", source))?;
    Ok((engine, sandbox))
}
