//! A live save that commits answers its receipt, a full turn store is a skip,
//! and the sink names the workspace a turn saves into.

use super::capture_tests::{turn, turn_refused};
use super::*;
use crate::run_loop::tests::recovery::{CHECKPOINT, archive::Refusal, server::Server};
use afr_agent::{CommittedRecovery, RecoveryReceipt};

#[tokio::test]
async fn a_live_durable_task_capture_commits_and_answers_its_receipt() -> Result<()> {
    let mut fixture = Fixture::new(|_| None)?;
    let mut server = Server::start(Refusal::None).await;
    let plane = server
        .plane
        .take()
        .ok_or_else(|| error::config("fixture plane missing"))?;
    Arc::get_mut(&mut fixture.rig.lessee)
        .ok_or_else(|| error::config("fixture lessee is shared"))?
        .plane = plane;
    let history = b"saved conversation".to_vec();
    let mut request = snapshot(&fixture)?;
    if let RecoveryState::DurableTask(task) = &mut request.state {
        task.history = Zeroizing::new(history.clone());
    }
    let run = fixture.run()?;
    let (_, mut owner) = BoundaryOwner::new(&run, Workspace::ApiOnly);

    let saved = owner.capture(request).await;

    let Ok(RecoverySave::Committed(receipt)) = saved else {
        return Err(error::config("the live save was not committed"));
    };
    let RecoveryReceipt {
        run_id,
        turn_id,
        fencing_token,
        checkpoint,
    } = *receipt;
    assert_eq!(run_id, test_support::RUN_ID);
    assert_eq!(turn_id, fixture.run.event.turn_id.as_ref());
    assert_eq!(fencing_token, test_support::FENCING);
    assert!(matches!(
        checkpoint,
        CommittedRecovery::DurableTask { checkpoint_id, generation: 1, history_digest }
            if checkpoint_id == CHECKPOINT
                && history_digest == af_core::etag::sha256_hex(&history)
    ));
    assert!(!run.interrupt.is_cancelled());
    server.stop();
    Ok(())
}

#[tokio::test]
async fn a_turn_capture_past_its_checkpoint_limit_is_skipped_and_the_run_goes_on() -> Result<()> {
    let limit: fn() -> crate::Error = || {
        error::refused(
            Verb::TurnCheckpoint,
            409,
            Some(af_core::error_code::RUN_CHECKPOINT_LIMIT),
        )
    };
    let mut fixture = Fixture::new(turn_refused(limit))?;
    let run = fixture.run()?;
    let (_, mut owner) = BoundaryOwner::new(&run, Workspace::ApiOnly);

    let saved = owner.capture(turn(&fixture)?).await;

    assert!(matches!(
        saved,
        Ok(RecoverySave::Skipped(RecoverySkip::Limit))
    ));
    assert!(!run.interrupt.is_cancelled());
    drop(owner);
    drop(run);
    assert_eq!(
        fixture
            .rig
            .calls()
            .iter()
            .filter(|call| call.verb == Verb::TurnCheckpoint)
            .count(),
        1,
        "only the allocation was asked for"
    );
    Ok(())
}

#[tokio::test]
async fn a_ready_sandbox_names_its_toolbox_and_one_without_one_is_unavailable() -> Result<()> {
    let fixture = Fixture::new(|_| None)?;
    let run = fixture.run()?;
    let root = tempfile::tempdir()?;
    for toolbox in [Some(api_toolbox()), None] {
        let engine = FakeEngine {
            workspace: Some(root.path().to_owned()),
            toolbox: toolbox.clone(),
            ..FakeEngine::default()
        };
        let sandbox = engine
            .prepare(SandboxRequest::new(test_support::RUN_ID, Limits::default()))
            .await
            .map_err(|source| error::boundary("fixture sandbox", source))?;
        let cell = OnceLock::from(sandbox);
        let state = OnceLock::new();
        let (sink, _owner) = BoundaryOwner::new(
            &run,
            Workspace::Building {
                cell: &cell,
                recovery: &state,
            },
        );

        let named = sink.workspace();

        match toolbox {
            Some(digest) => assert_eq!(
                named,
                Ok(af_wire::run::TurnWorkspace::Files {
                    toolbox_digest: digest
                })
            ),
            None => assert_eq!(named, Err(RecoverySkip::Unavailable)),
        }
        assert!(
            matches!(sink.try_reserve(false), Ok(Some(_))),
            "a ready sandbox reserves file storage"
        );
    }
    Ok(())
}

#[tokio::test]
async fn an_api_only_durable_task_reserves_file_storage_until_none_is_left() -> Result<()> {
    let fixture = Fixture::new(|_| None)?;
    let run = fixture.run()?;
    let (sink, _owner) = BoundaryOwner::new(&run, Workspace::ApiOnly);

    assert!(
        matches!(sink.try_reserve(false), Ok(None)),
        "a turn needs none"
    );
    let mut held = Vec::new();
    let refused = loop {
        match sink.try_reserve(true) {
            Ok(Some(permit)) => held.push(permit),
            Ok(None) => return Err(error::config("a task reserved nothing")),
            Err(skip) => break skip,
        }
    };

    assert_eq!(refused, RecoverySkip::BusyStorage);
    assert_eq!(
        held.len(),
        crate::config::RuntimePolicy::DEFAULT
            .settings()
            .max_concurrent_file_operations
    );
    Ok(())
}
