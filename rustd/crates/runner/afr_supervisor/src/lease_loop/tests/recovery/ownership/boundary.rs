//! Failed saves, lost callers and cleanup failures never fabricate a receipt.

use super::*;
use crate::lease_loop::recovery_boundary::{BoundaryOwner, Workspace};
use afr_agent::{
    RecoveryBoundarySink as _, RecoverySave, RecoverySkip, RecoverySnapshot, RecoveryState,
};
use std::time::Duration;

#[tokio::test]
async fn an_oversized_ordinary_workspace_skips_after_thaw_without_stopping() -> Result<()> {
    let mut fixture = Fixture::new(unavailable)?;
    fixture.lease.durable_tasks = false;
    let run = fixture.run()?;
    let root = tempfile::tempdir()?;
    std::fs::File::create(root.path().join("large"))?.set_len(24 * 1024 * 1024 * 1024 + 1)?;
    let (engine, sandbox) = sandbox(root.path(), Freezer::Works).await?;
    let cell = OnceLock::from(sandbox);
    let state = OnceLock::new();
    let (_, mut owner) = BoundaryOwner::new(
        &run,
        Workspace::Building {
            cell: &cell,
            recovery: &state,
        },
    );
    let mut request = snapshot(&fixture)?;
    request.state = RecoveryState::Turn(afr_agent::TurnSnapshot {
        scope: af_wire::lease::TurnScope {
            workspace_id: af_wire::lease::TurnId(fixture.lease.event.workspace_id.to_string()),
            fleet_id: af_wire::lease::TurnId(fixture.lease.event.fleet_id.to_string()),
            event_id: af_wire::lease::TurnId(fixture.lease.event.event_id.to_string()),
            setup_revision_id: af_wire::lease::TurnId(ID.to_owned()),
            workspace: af_wire::lease::TurnWorkspace::Files {
                toolbox_digest: "a".repeat(64),
            },
        },
        previous: None,
        history: zeroize::Zeroizing::new(Vec::new()),
        folds: Vec::new(),
    });
    assert!(matches!(
        owner.capture(request).await,
        Ok(RecoverySave::Skipped(RecoverySkip::Limit))
    ));
    assert_eq!(engine.thawed.load(Ordering::SeqCst), 1);
    assert!(!run.interrupt.is_cancelled());
    drop(owner);
    drop(run);
    assert_eq!(fixture.rig.calls().len(), 0);
    Ok(())
}

fn snapshot(fixture: &Fixture) -> Result<RecoverySnapshot> {
    Ok(RecoverySnapshot {
        lease_id: fixture.lease.lease_id.to_string(),
        event_id: fixture.lease.event.event_id.to_string(),
        fencing_token: fixture.lease.fencing_token,
        file_permit: None,
        state: RecoveryState::Task(TaskRecovery {
            task: fixture
                .prepared
                .inputs
                .current_task
                .clone()
                .ok_or_else(|| error::config("fixture task missing"))?,
            history: zeroize::Zeroizing::new(Vec::new()),
        }),
    })
}

fn unavailable(call: &Call) -> Option<Answer> {
    (call.verb == Verb::TaskCheckpointPlan).then(|| {
        Answer::Fail(error::refused(
            call.verb,
            503,
            Some(af_core::error_code::TASK_STORAGE_UNAVAILABLE),
        ))
    })
}

#[tokio::test]
async fn building_workspace_cannot_publish_api_only_history() -> Result<()> {
    let mut fixture = Fixture::new(unavailable)?;
    let run = fixture.run()?;
    let cell = OnceLock::new();
    let state = OnceLock::new();
    let (_, mut owner) = BoundaryOwner::new(
        &run,
        Workspace::Building {
            cell: &cell,
            recovery: &state,
        },
    );
    assert!(matches!(
        owner.capture(snapshot(&fixture)?).await,
        Ok(RecoverySave::Skipped(RecoverySkip::WorkspaceBuilding))
    ));
    drop(owner);
    drop(run);
    assert_eq!(
        fixture.rig.calls().len(),
        0,
        "no native I/O precedes this boundary"
    );
    Ok(())
}

#[tokio::test]
async fn a_foreign_snapshot_cancels_before_native_io() -> Result<()> {
    let mut fixture = Fixture::new(unavailable)?;
    for field in 0..3 {
        let run = fixture.run()?;
        let (_, mut owner) = BoundaryOwner::new(&run, Workspace::ApiOnly);
        let mut snapshot = snapshot(&fixture)?;
        match field {
            0 => snapshot.lease_id = PAGE.to_owned(),
            1 => snapshot.event_id = PAGE.to_owned(),
            _ => snapshot.fencing_token += 1,
        }
        assert!(
            owner.capture(snapshot).await.is_err_and(
                |failure| failure.code() == af_core::error_code::INTERNAL_OPERATION_FAILED
            )
        );
        assert!(run.interrupt.is_cancelled());
    }
    assert_eq!(
        fixture.rig.calls().len(),
        0,
        "no native I/O precedes this boundary"
    );
    Ok(())
}

#[tokio::test]
async fn a_refused_or_busy_freeze_still_thaws_without_a_native_save() -> Result<()> {
    for (freezer, reason) in [
        (Freezer::RefusesFreeze, RecoverySkip::Unavailable),
        (Freezer::BusyTenant, RecoverySkip::BusyProcesses),
    ] {
        let mut fixture = Fixture::new(unavailable)?;
        let run = fixture.run()?;
        let root = tempfile::tempdir()?;
        let (engine, sandbox) = sandbox(root.path(), freezer).await?;
        let cell = OnceLock::from(sandbox);
        let state = OnceLock::new();
        let (_, mut owner) = BoundaryOwner::new(
            &run,
            Workspace::Building {
                cell: &cell,
                recovery: &state,
            },
        );
        assert!(matches!(owner.capture(snapshot(&fixture)?).await,
            Ok(RecoverySave::Skipped(found)) if found == reason));
        assert_eq!(engine.thaw_attempts.load(Ordering::SeqCst), 1);
        assert_eq!(engine.thawed.load(Ordering::SeqCst), 1);
        assert!(!run.interrupt.is_cancelled());
        drop(owner);
        drop(run);
        assert_eq!(
            fixture.rig.calls().len(),
            0,
            "no native I/O precedes this boundary"
        );
    }
    Ok(())
}

#[tokio::test]
async fn failed_thaw_cancels_the_run_even_when_storage_was_only_unavailable() -> Result<()> {
    let fixture = Fixture::new(unavailable)?;
    let run = fixture.run()?;
    let root = tempfile::tempdir()?;
    let (engine, sandbox) = sandbox(root.path(), Freezer::RefusesThaw).await?;
    let cell = OnceLock::from(sandbox);
    let state = OnceLock::new();
    let (_, mut owner) = BoundaryOwner::new(
        &run,
        Workspace::Building {
            cell: &cell,
            recovery: &state,
        },
    );
    assert!(
        owner
            .capture(snapshot(&fixture)?)
            .await
            .is_err_and(|failure| failure.code() == af_core::error_code::INTERNAL_OPERATION_FAILED)
    );
    assert!(run.interrupt.is_cancelled());
    assert_eq!(engine.frozen.load(Ordering::SeqCst), 1);
    assert_eq!(engine.thaw_attempts.load(Ordering::SeqCst), 1);
    assert_eq!(engine.thawed.load(Ordering::SeqCst), 0);
    Ok(())
}

#[tokio::test]
async fn an_abandoned_executor_file_job_prevents_freeze_and_publication() -> Result<()> {
    let mut fixture = Fixture::new(unavailable)?;
    let run = fixture.run()?;
    let root = tempfile::tempdir()?;
    let (engine, sandbox) = sandbox(root.path(), Freezer::BusyExecutor).await?;
    let cell = OnceLock::from(sandbox);
    let state = OnceLock::new();
    let (_, mut owner) = BoundaryOwner::new(
        &run,
        Workspace::Building {
            cell: &cell,
            recovery: &state,
        },
    );
    assert!(matches!(
        owner.capture(snapshot(&fixture)?).await,
        Ok(RecoverySave::Skipped(RecoverySkip::BusyExecutor))
    ));
    assert_eq!(engine.frozen.load(Ordering::SeqCst), 0);
    assert_eq!(engine.thaw_attempts.load(Ordering::SeqCst), 0);
    drop(owner);
    drop(run);
    assert_eq!(
        fixture.rig.calls().len(),
        0,
        "unfinished file work cannot publish"
    );
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn a_vanished_agent_cannot_release_the_owner_before_thaw_finishes() -> Result<()> {
    let fixture = Fixture::new(unavailable)?;
    let run = fixture.run()?;
    let root = tempfile::tempdir()?;
    let engine = FakeEngine {
        workspace: Some(root.path().to_owned()),
        toolbox: Some(api_toolbox()),
        thaw_takes: Duration::from_secs(10),
        ..FakeEngine::default()
    };
    let sandbox = engine
        .prepare(SandboxRequest::new(
            test_support::LEASE_ID,
            Limits::default(),
        ))
        .await
        .map_err(|source| error::boundary("fixture sandbox", source))?;
    let cell = OnceLock::from(sandbox);
    let state = OnceLock::new();
    let (sink, owner) = BoundaryOwner::new(
        &run,
        Workspace::Building {
            cell: &cell,
            recovery: &state,
        },
    );
    let snapshot = snapshot(&fixture)?;
    let work = async {
        let saving = sink.save(snapshot);
        tokio::pin!(saving);
        tokio::select! {
            _result = &mut saving => panic!("the caller must disappear during thaw"),
            () = async {
                while engine.thaw_attempts.load(Ordering::SeqCst) == 0 {
                    tokio::task::yield_now().await;
                }
            } => {}
        }
        crate::lease_loop::failed(
            af_wire::report::FailureClass::RunnerCrash,
            "caller disappeared",
        )
    };
    let started = tokio::time::Instant::now();
    let _ending = owner.beside(work).await;
    assert!(started.elapsed() >= engine.thaw_takes);
    assert_eq!(engine.thawed.load(Ordering::SeqCst), 1);
    Ok(())
}

#[path = "capture.rs"]
mod capture_tests;
