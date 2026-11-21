//! A frozen sandbox and an API-only run share authenticated checkpoint ownership.

use super::*;
use crate::{
    client::{Call, Verb},
    egress::{Bound, Egress},
    holds::{BuiltUnder, HoldKey},
    run_loop::{Ids, segments::Timeline},
    test_support::{self, Answer, Behaviour, FakeAgent, FakeEngine, Freezer, Rig},
};
use af_wire::{
    run::RunPayload,
    task::{CheckpointAuthorizations, CheckpointPage, TaskContext, TaskInputsResponse},
};
use afr_agent::Meter;
use afr_sandbox::{Engine as _, Limits, SandboxRequest};
use std::sync::{Arc, OnceLock, atomic::Ordering};
use tokio_util::sync::CancellationToken;

const ID: &str = "019f0000-0000-7000-8000-000000000001";
const PAGE: &str = "019f0000-0000-7000-8000-000000000002";

#[path = "ownership/fixture.rs"]
mod fixture;
use fixture::{Fixture, saved};

fn history(state: Option<&TaskRecovery>) -> Result<&TaskRecovery> {
    state.ok_or_else(|| error::config("fixture expected authenticated recovery"))
}

#[tokio::test]
async fn api_recovery_uses_manifest_cursors_and_current_task_identity() -> Result<()> {
    let fixture = Fixture::new(saved)?;
    let restored = fixture
        .run()?
        .recover_without_sandbox(Some(&fixture.prepared))
        .await?;
    let restored = history(restored.as_ref())?;
    assert_eq!(restored.task.task_id, ID);
    assert_eq!(restored.task.generation, 7);
    assert_eq!(restored.task.covered_input_sequence, 3);
    assert_eq!(restored.task.confirmed_effect_watermark, 2);
    assert_eq!(restored.task.input_sequence, 7);
    assert_eq!(restored.history.as_slice(), b"");
    Ok(())
}

#[tokio::test]
async fn frozen_recovery_installs_history_only_after_a_successful_thaw() -> Result<()> {
    for freezer in [Freezer::Works, Freezer::RefusesThaw] {
        let fixture = Fixture::new(saved)?;
        let root = tempfile::tempdir()?;
        let (engine, sandbox) = sandbox(root.path(), freezer).await?;
        let owner = OnceLock::new();
        let result = fixture
            .run()?
            .prepare_recovery(sandbox.as_ref(), &fixture.build(&owner))
            .await;
        assert_eq!(engine.frozen.load(Ordering::SeqCst), 1);
        assert_eq!(engine.thaw_attempts.load(Ordering::SeqCst), 1);
        if freezer == Freezer::Works {
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
            assert_eq!(engine.thawed.load(Ordering::SeqCst), 1);
        } else {
            let Err(failure) = result else {
                return Err(error::config("failed thaw was accepted"));
            };
            assert!(
                matches!(failure.kind(), error::ErrorKind::Boundary { detail, .. }
                if *detail == "sandbox could not thaw after recovery")
            );
            let mut cause = std::error::Error::source(&failure);
            let mut freezer_cause = false;
            while let Some(source) = cause {
                freezer_cause |= source.to_string().contains(test_support::NO_FREEZER);
                cause = source.source();
            }
            assert!(freezer_cause, "{failure:?}");
            assert!(owner.get().is_none());
            assert_eq!(engine.thawed.load(Ordering::SeqCst), 0);
        }
    }
    Ok(())
}

#[tokio::test]
async fn a_restore_refusal_remains_primary_when_thaw_also_fails() -> Result<()> {
    let fixture = Fixture::new(|call| {
        (call.verb == Verb::TaskCheckpointAuthorizations).then(|| {
            Answer::Fail(error::refused(
                call.verb,
                409,
                Some(af_core::error_code::TASK_STALE_STATE),
            ))
        })
    })?;
    let root = tempfile::tempdir()?;
    let (engine, sandbox) = sandbox(root.path(), Freezer::RefusesThaw).await?;
    let owner = OnceLock::new();
    let result = fixture
        .run()?
        .prepare_recovery(sandbox.as_ref(), &fixture.build(&owner))
        .await;
    let Err(failure) = result else {
        return Err(error::config("restore refusal was lost"));
    };
    assert_eq!(failure.code(), af_core::error_code::TASK_STALE_STATE);
    assert!(owner.get().is_none());
    assert_eq!(engine.frozen.load(Ordering::SeqCst), 1);
    assert_eq!(engine.thaw_attempts.load(Ordering::SeqCst), 1);
    assert_eq!(engine.thawed.load(Ordering::SeqCst), 0);
    assert_eq!(std::fs::read_dir(root.path())?.count(), 0);
    Ok(())
}

#[tokio::test]
async fn repository_recovery_without_a_sandbox_has_no_native_read() -> Result<()> {
    let mut fixture = Fixture::new(|_| None)?;
    let task = fixture
        .prepared
        .inputs
        .current_task
        .as_mut()
        .ok_or_else(|| error::config("fixture task missing"))?;
    task.target = TaskTarget::ExistingPr {
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
    fixture.run.task = fixture.prepared.inputs.current_task.clone();
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

async fn sandbox(
    root: &std::path::Path,
    freezer: Freezer,
) -> Result<(FakeEngine, Box<dyn Sandbox>)> {
    let engine = FakeEngine {
        workspace: Some(root.to_owned()),
        toolbox: Some(api_toolbox()),
        freezer,
        ..FakeEngine::default()
    };
    let sandbox = engine
        .prepare(SandboxRequest::new(test_support::RUN_ID, Limits::default()))
        .await
        .map_err(|source| error::boundary("fixture sandbox", source))?;
    Ok((engine, sandbox))
}
