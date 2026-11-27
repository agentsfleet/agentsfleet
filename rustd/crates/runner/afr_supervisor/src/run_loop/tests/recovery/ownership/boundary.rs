//! Failed saves, lost callers and cleanup failures never fabricate a receipt.

use super::*;
use crate::run_loop::recovery_boundary::{BoundaryOwner, Workspace};
use afr_agent::{
    RecoveryBoundarySink as _, RecoverySave, RecoverySkip, RecoverySnapshot, RecoveryState,
};
use std::time::Duration;

fn snapshot(fixture: &Fixture) -> Result<RecoverySnapshot> {
    Ok(RecoverySnapshot {
        run_id: fixture.run.run_id.to_string(),
        turn_id: fixture.run.event.turn_id.to_string(),
        fencing_token: fixture.run.fencing_token,
        file_permit: None,
        state: RecoveryState::DurableTask(DurableTaskRecovery {
            task: fixture
                .prepared
                .inputs
                .current_durable_task
                .clone()
                .ok_or_else(|| error::config("fixture task missing"))?,
            history: Zeroizing::new(Vec::new()),
        }),
    })
}

fn unavailable(call: &Call) -> Option<Answer> {
    (call.verb == Verb::DurableTaskCheckpointPlan).then(|| {
        Answer::Fail(error::refused(
            call.verb,
            503,
            Some(af_core::error_code::DURABLE_TASK_STORAGE_UNAVAILABLE),
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
            0 => snapshot.run_id = PAGE.to_owned(),
            1 => snapshot.turn_id = PAGE.to_owned(),
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
async fn a_busy_tenant_skips_the_boundary_without_a_native_save() -> Result<()> {
    let mut fixture = Fixture::new(unavailable)?;
    let run = fixture.run()?;
    let root = tempfile::tempdir()?;
    let (engine, sandbox) = sandbox(root.path(), Parking::BusyTenant).await?;
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
        Ok(RecoverySave::Skipped(RecoverySkip::BusyProcesses))
    ));
    assert_eq!(
        engine.parked.load(Ordering::SeqCst),
        0,
        "a capture parks nothing"
    );
    assert!(!run.interrupt.is_cancelled());
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
async fn an_abandoned_agent_file_job_prevents_publication() -> Result<()> {
    let mut fixture = Fixture::new(unavailable)?;
    let run = fixture.run()?;
    let root = tempfile::tempdir()?;
    let (engine, sandbox) = sandbox(root.path(), Parking::BusyAgent).await?;
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
        Ok(RecoverySave::Skipped(RecoverySkip::BusyAgent))
    ));
    assert_eq!(engine.parked.load(Ordering::SeqCst), 0);
    assert_eq!(
        engine.quiesced.load(Ordering::SeqCst),
        0,
        "busy file work is refused before the tenant is read"
    );
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
async fn a_vanished_agent_cannot_release_the_owner_before_quiescence_is_read() -> Result<()> {
    let fixture = Fixture::new(unavailable)?;
    let run = fixture.run()?;
    let root = tempfile::tempdir()?;
    let engine = FakeEngine {
        workspace: Some(root.path().to_owned()),
        toolbox: Some(api_toolbox()),
        quiesce_takes: Duration::from_secs(10),
        ..FakeEngine::default()
    };
    let sandbox = engine
        .sandbox()
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
            _result = &mut saving => panic!("the caller must disappear while quiescence is read"),
            () = async {
                while engine.quiesced.load(Ordering::SeqCst) == 0 {
                    tokio::task::yield_now().await;
                }
            } => {}
        }
        crate::run_loop::failed(
            af_wire::report::FailureClass::RunnerCrash,
            "caller disappeared",
        )
    };
    let started = tokio::time::Instant::now();
    let _ending = owner.beside(work).await;
    assert!(started.elapsed() >= engine.quiesce_takes);
    assert_eq!(
        engine.parked.load(Ordering::SeqCst),
        0,
        "a capture parks nothing"
    );
    Ok(())
}

#[path = "capture.rs"]
mod capture_tests;

#[path = "live.rs"]
mod live_tests;
