//! A live capture skips what it cannot prove safe, stops the run on what it
//! cannot trust, and never leaves a waiting save without an answer.

use super::*;
use af_wire::report::FailureClass;
use af_wire::run::{CheckpointOperationId, TurnScope, TurnWorkspace};
use futures_util::FutureExt as _;

/// A run whose native owner answers every turn checkpoint with `failure`.
fn turn_refused(failure: fn() -> crate::Error) -> impl Fn(&Call) -> Option<Answer> {
    move |call| (call.verb == Verb::TurnCheckpoint).then(|| Answer::Fail(failure()))
}

/// A run whose native owner refuses the task checkpoint plan with `code`.
fn plan_refused(code: af_core::error_code::ErrorCode) -> impl Fn(&Call) -> Option<Answer> {
    move |call| {
        (call.verb == Verb::TaskCheckpointPlan)
            .then(|| Answer::Fail(error::refused(call.verb, 409, Some(code))))
    }
}

/// An API-only ordinary turn of the fixture's run.
fn turn(fixture: &Fixture) -> Result<RecoverySnapshot> {
    let mut request = snapshot(fixture)?;
    request.state = RecoveryState::Turn(afr_agent::TurnSnapshot {
        scope: TurnScope {
            workspace_id: CheckpointOperationId(fixture.run.event.workspace_id.to_string()),
            fleet_id: CheckpointOperationId(fixture.run.event.fleet_id.to_string()),
            event_id: CheckpointOperationId(fixture.run.event.event_id.to_string()),
            setup_revision_id: CheckpointOperationId(ID.to_owned()),
            workspace: TurnWorkspace::ApiOnly,
        },
        previous: None,
        history: zeroize::Zeroizing::new(b"saved conversation".to_vec()),
        folds: Vec::new(),
    });
    Ok(request)
}

fn turn_commands(calls: &[Call]) -> usize {
    calls
        .iter()
        .filter(|call| call.verb == Verb::TurnCheckpoint)
        .count()
}

#[tokio::test]
async fn a_repository_task_without_a_ready_sandbox_stops_the_run_before_native_io() -> Result<()> {
    let mut fixture = Fixture::new(unavailable)?;
    let run = fixture.run()?;
    let (_, mut owner) = BoundaryOwner::new(&run, Workspace::ApiOnly);
    let mut request = snapshot(&fixture)?;
    if let RecoveryState::Task(task) = &mut request.state {
        task.task.target = TaskTarget::ExistingPr {
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
    }

    let saved = owner.capture(request).await;

    assert!(
        saved
            .is_err_and(|failure| failure.code() == af_core::error_code::INTERNAL_OPERATION_FAILED)
    );
    assert!(run.interrupt.is_cancelled());
    drop(owner);
    drop(run);
    assert_eq!(
        fixture.rig.calls().len(),
        0,
        "no native I/O precedes refusal"
    );
    Ok(())
}

#[tokio::test]
async fn a_sandbox_without_a_workspace_or_toolbox_skips_after_thaw() -> Result<()> {
    let root = tempfile::tempdir()?;
    for engine in [
        FakeEngine {
            toolbox: Some(api_toolbox()),
            ..FakeEngine::default()
        },
        FakeEngine {
            workspace: Some(root.path().to_owned()),
            ..FakeEngine::default()
        },
    ] {
        let mut fixture = Fixture::new(unavailable)?;
        let run = fixture.run()?;
        let sandbox = engine
            .prepare(SandboxRequest::new(test_support::RUN_ID, Limits::default()))
            .await
            .map_err(|source| error::boundary("fixture sandbox", source))?;
        let cell = OnceLock::from(sandbox);
        let state = OnceLock::new();
        let (_, mut owner) = BoundaryOwner::new(
            &run,
            Workspace::Building {
                cell: &cell,
                recovery: &state,
            },
        );

        let saved = owner.capture(snapshot(&fixture)?).await;

        assert!(matches!(
            saved,
            Ok(RecoverySave::Skipped(RecoverySkip::Unavailable))
        ));
        assert_eq!(engine.frozen.load(Ordering::SeqCst), 1);
        assert_eq!(engine.thawed.load(Ordering::SeqCst), 1);
        assert!(!run.interrupt.is_cancelled());
        drop(owner);
        drop(run);
        assert_eq!(fixture.rig.calls().len(), 0, "nothing is published");
    }
    Ok(())
}

#[tokio::test]
async fn an_executor_that_cannot_prove_idle_skips_without_freezing() -> Result<()> {
    let mut fixture = Fixture::new(unavailable)?;
    let run = fixture.run()?;
    let root = tempfile::tempdir()?;
    let (engine, sandbox) = sandbox(root.path(), Freezer::ExecutorUnproved).await?;
    let cell = OnceLock::from(sandbox);
    let state = OnceLock::new();
    let (_, mut owner) = BoundaryOwner::new(
        &run,
        Workspace::Building {
            cell: &cell,
            recovery: &state,
        },
    );

    let saved = owner.capture(snapshot(&fixture)?).await;

    assert!(matches!(
        saved,
        Ok(RecoverySave::Skipped(RecoverySkip::Unavailable))
    ));
    assert_eq!(engine.frozen.load(Ordering::SeqCst), 0);
    assert_eq!(engine.thaw_attempts.load(Ordering::SeqCst), 0);
    assert!(!run.interrupt.is_cancelled());
    drop(owner);
    drop(run);
    assert_eq!(fixture.rig.calls().len(), 0);
    Ok(())
}

#[tokio::test]
async fn an_unprovable_tenant_stops_the_run_after_thaw() -> Result<()> {
    let mut fixture = Fixture::new(unavailable)?;
    let run = fixture.run()?;
    let root = tempfile::tempdir()?;
    let (engine, sandbox) = sandbox(root.path(), Freezer::QuiescenceUnproved).await?;
    let cell = OnceLock::from(sandbox);
    let state = OnceLock::new();
    let (_, mut owner) = BoundaryOwner::new(
        &run,
        Workspace::Building {
            cell: &cell,
            recovery: &state,
        },
    );

    let saved = owner.capture(snapshot(&fixture)?).await;

    assert!(
        saved
            .is_err_and(|failure| failure.code() == af_core::error_code::INTERNAL_OPERATION_FAILED)
    );
    assert!(run.interrupt.is_cancelled());
    assert_eq!(engine.frozen.load(Ordering::SeqCst), 1);
    assert_eq!(engine.thawed.load(Ordering::SeqCst), 1);
    drop(owner);
    drop(run);
    assert_eq!(fixture.rig.calls().len(), 0, "nothing is published");
    Ok(())
}

#[tokio::test]
async fn a_task_capture_at_its_limit_skips_and_a_stale_one_stops_the_run() -> Result<()> {
    for (code, skipped) in [
        (af_core::error_code::TASK_LIMIT_EXHAUSTED, true),
        (af_core::error_code::TASK_STALE_STATE, false),
    ] {
        let mut fixture = Fixture::new(plan_refused(code))?;
        let run = fixture.run()?;
        let (_, mut owner) = BoundaryOwner::new(&run, Workspace::ApiOnly);

        let saved = owner.capture(snapshot(&fixture)?).await;

        if skipped {
            assert!(matches!(
                saved,
                Ok(RecoverySave::Skipped(RecoverySkip::Limit))
            ));
            assert!(!run.interrupt.is_cancelled());
        } else {
            assert!(saved.is_err_and(|failure| failure.code() == code));
            assert!(run.interrupt.is_cancelled());
        }
        drop(owner);
        drop(run);
        assert!(
            fixture
                .rig
                .calls()
                .iter()
                .any(|call| call.verb == Verb::TaskCheckpointPlan),
            "the plan was asked for"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_conflicting_turn_capture_skips_and_a_refused_one_stops_the_run() -> Result<()> {
    let conflict: fn() -> crate::Error = || {
        error::refused(
            Verb::TurnCheckpoint,
            409,
            Some(af_core::error_code::RUN_CHECKPOINT_CONFLICT),
        )
    };
    let refused: fn() -> crate::Error = || error::config("the owner refused this capture");
    for (failure, skipped) in [(conflict, true), (refused, false)] {
        let mut fixture = Fixture::new(turn_refused(failure))?;
        let run = fixture.run()?;
        let (_, mut owner) = BoundaryOwner::new(&run, Workspace::ApiOnly);

        let saved = owner.capture(turn(&fixture)?).await;

        if skipped {
            assert!(matches!(
                saved,
                Ok(RecoverySave::Skipped(RecoverySkip::Unavailable))
            ));
            assert!(!run.interrupt.is_cancelled());
        } else {
            assert!(saved.is_err_and(
                |failure| failure.code() == af_core::error_code::INTERNAL_OPERATION_FAILED
            ));
            assert!(run.interrupt.is_cancelled());
        }
        drop(owner);
        drop(run);
        assert_eq!(
            turn_commands(&fixture.rig.calls()),
            1,
            "only the allocation was asked for"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_terminal_capture_reserves_storage_only_for_an_idle_executor() -> Result<()> {
    let fixture = Fixture::new(unavailable)?;
    let run = fixture.run()?;
    let root = tempfile::tempdir()?;

    let (_, idle) = sandbox(root.path(), Freezer::Works).await?;
    assert!(matches!(
        run.reserve_terminal_capture(idle.as_ref(), false).await,
        Ok(None)
    ));
    assert!(matches!(
        run.reserve_terminal_capture(idle.as_ref(), true).await,
        Ok(Some(_))
    ));

    let (_, busy) = sandbox(root.path(), Freezer::BusyExecutor).await?;
    let refused = run.reserve_terminal_capture(busy.as_ref(), true).await;
    assert_eq!(
        test_support::detail(&refused),
        Some("terminal capture has active file work")
    );
    assert!(
        refused.is_err_and(
            |failure| failure.code() == af_core::error_code::RUN_CHECKPOINT_UNAVAILABLE
        )
    );

    let (_, unproved) = sandbox(root.path(), Freezer::ExecutorUnproved).await?;
    let refused = run.reserve_terminal_capture(unproved.as_ref(), true).await;
    assert_eq!(
        test_support::detail(&refused),
        Some("cannot prove executor file jobs idle")
    );
    Ok(())
}

#[tokio::test]
async fn a_building_workspace_reserves_no_storage() -> Result<()> {
    let fixture = Fixture::new(unavailable)?;
    let run = fixture.run()?;
    let cell = OnceLock::new();
    let state = OnceLock::new();
    let (sink, _owner) = BoundaryOwner::new(
        &run,
        Workspace::Building {
            cell: &cell,
            recovery: &state,
        },
    );

    for task in [false, true] {
        assert!(matches!(
            sink.try_reserve(task),
            Err(RecoverySkip::WorkspaceBuilding)
        ));
    }
    Ok(())
}

#[tokio::test]
async fn a_save_after_its_owner_is_gone_fails_instead_of_waiting() -> Result<()> {
    let mut fixture = Fixture::new(unavailable)?;
    let run = fixture.run()?;
    let (sink, owner) = BoundaryOwner::new(&run, Workspace::ApiOnly);
    drop(owner);

    let saved = sink.save(snapshot(&fixture)?).await;

    assert!(
        saved
            .is_err_and(|failure| failure.code() == af_core::error_code::INTERNAL_OPERATION_FAILED)
    );
    drop(sink);
    drop(run);
    assert_eq!(fixture.rig.calls().len(), 0);
    Ok(())
}

#[tokio::test]
async fn a_save_queued_as_the_work_ends_is_answered_unavailable() -> Result<()> {
    let mut fixture = Fixture::new(unavailable)?;
    let run = fixture.run()?;
    let (sink, owner) = BoundaryOwner::new(&run, Workspace::ApiOnly);
    let mut saving = sink.save(snapshot(&fixture)?);
    assert!(
        (&mut saving).now_or_never().is_none(),
        "the save waits on its queued job"
    );

    let _ending = owner
        .beside(async { crate::run_loop::failed(FailureClass::RunnerCrash, "work ended") })
        .await;

    assert!(matches!(
        saving.await,
        Ok(RecoverySave::Skipped(RecoverySkip::Unavailable))
    ));
    drop(sink);
    drop(run);
    assert_eq!(fixture.rig.calls().len(), 0, "the queued job never ran");
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn a_save_whose_owner_is_dropped_mid_capture_reports_a_lost_reply() -> Result<()> {
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
        .prepare(SandboxRequest::new(test_support::RUN_ID, Limits::default()))
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
    let mut beside = Box::pin(owner.beside(std::future::pending()));
    let mut saving = sink.save(snapshot(&fixture)?);

    tokio::select! {
        _ending = &mut beside => return Err(error::config("the work never ends")),
        _saved = &mut saving => return Err(error::config("the save must wait for thaw")),
        () = async {
            while engine.thaw_attempts.load(Ordering::SeqCst) == 0 {
                tokio::task::yield_now().await;
            }
        } => {}
    }
    drop(beside);

    assert!(
        saving
            .await
            .is_err_and(|failure| failure.code() == af_core::error_code::INTERNAL_OPERATION_FAILED)
    );
    Ok(())
}
