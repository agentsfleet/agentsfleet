//! A finished run's files and history commit as one checkpoint before its
//! sandbox parks, and a capture that cannot prove its owner, workspace or
//! tenant blocks the task.

use super::prepare_tests::pull_request;
use super::*;
use crate::run_loop::tests::recovery::{CHECKPOINT, archive::Refusal, server::Server};
use af_wire::durable_task::{DecisionDurableTaskVersion, DurableTaskReport};

/// What a run hands back with history to save and a waiting task.
fn ending(fixture: &Fixture) -> Result<Ending> {
    let task = fixture
        .prepared
        .inputs
        .current_durable_task
        .clone()
        .ok_or_else(|| error::config("fixture task missing"))?;
    let mut output = test_support::answer();
    output.task = Some(DurableTaskReport {
        durable_task: DecisionDurableTaskVersion {
            durable_task_id: task.durable_task_id.to_string(),
            generation: task.generation,
        },
        disposition: DurableTaskDisposition::Waiting,
        decision_call_id: Some(ID.to_owned()),
        checkpoint_id: None,
        covered_input_sequence: 7,
        effect_watermark: 5,
    });
    output.recovery = Some(DurableTaskRecovery {
        task,
        history: Zeroizing::new(b"saved conversation".to_vec()),
    });
    Ok(Ending::Ran {
        output: Box::new(output),
        first_chunk: None,
    })
}

fn report(ending: &Ending) -> Result<&afr_agent::RunOutput> {
    let Ending::Ran { output, .. } = ending else {
        return Err(error::config("the run no longer ran"));
    };
    Ok(output)
}

/// A checkpoint server refusing nothing, its plane handed to `fixture`'s
/// lessee.
async fn serving(fixture: &mut Fixture) -> Result<Server> {
    let mut server = Server::start(Refusal::None).await;
    let plane = server
        .plane
        .take()
        .ok_or_else(|| error::config("fixture plane missing"))?;
    Arc::get_mut(&mut fixture.rig.lessee)
        .ok_or_else(|| error::config("fixture lessee is shared"))?
        .plane = plane;
    Ok(server)
}

const fn owner() -> State {
    State {
        history: None,
        turn: None,
        base: None,
        base_paths: Vec::new(),
    }
}

#[tokio::test]
async fn a_final_capture_commits_history_and_names_its_checkpoint_in_the_report() -> Result<()> {
    let mut fixture = Fixture::new(|_| None)?;
    let server = serving(&mut fixture).await?;
    let root = tempfile::tempdir()?;
    let (engine, sandbox) = sandbox(root.path(), Parking::Works).await?;
    let mut ending = ending(&fixture)?;

    fixture
        .run()?
        .capture_final(sandbox.as_ref(), Some(&owner()), &mut ending)
        .await;

    let output = report(&ending)?;
    assert!(output.recovery.is_none(), "the saved history is released");
    let task = output
        .task
        .as_ref()
        .ok_or_else(|| error::config("task report missing"))?;
    assert_eq!(task.checkpoint_id.as_deref(), Some(CHECKPOINT));
    assert_eq!(task.disposition, DurableTaskDisposition::Waiting);
    assert_eq!(task.decision_call_id.as_deref(), Some(ID));
    assert_eq!(output.result.content, "done");
    assert_eq!(
        engine.parked.load(Ordering::SeqCst),
        0,
        "the capture reads a serving sandbox and parks nothing"
    );
    let archive = server
        .archive
        .lock()
        .map_err(|_poisoned| error::config("archive poisoned"))?;
    let plan = archive
        .plan
        .as_ref()
        .ok_or_else(|| error::config("no checkpoint was planned"))?;
    assert!(plan.manifest_root.is_some(), "the page was committed");
    assert!(plan.history_digest.is_some(), "the history was committed");
    assert!(
        archive
            .calls
            .iter()
            .any(|(path, _)| path.ends_with(CHECKPOINT)),
        "the checkpoint was committed"
    );
    drop(archive);
    server.stop();
    Ok(())
}

#[tokio::test]
async fn a_final_capture_it_cannot_prove_blocks_the_durable_task_without_native_io() -> Result<()> {
    for fault in ["owner", "workspace", "toolbox", "unproved", "busy"] {
        let mut fixture = Fixture::new(|_| None)?;
        let root = tempfile::tempdir()?;
        let engine = FakeEngine {
            workspace: (fault != "workspace").then(|| root.path().to_owned()),
            toolbox: (fault != "toolbox").then(api_toolbox),
            parking: match fault {
                "unproved" => Parking::QuiescenceUnproved,
                "busy" => Parking::BusyTenant,
                _ => Parking::Works,
            },
            ..FakeEngine::default()
        };
        let sandbox = engine
            .sandbox()
            .await
            .map_err(|source| error::boundary("fixture sandbox", source))?;
        let state = owner();
        let mut ending = ending(&fixture)?;

        fixture
            .run()?
            .capture_final(
                sandbox.as_ref(),
                (fault != "owner").then_some(&state),
                &mut ending,
            )
            .await;

        let output = report(&ending)?;
        assert!(output.recovery.is_none(), "{fault}: history is released");
        let task = output
            .task
            .as_ref()
            .ok_or_else(|| error::config("task report missing"))?;
        assert_eq!(task.disposition, DurableTaskDisposition::Blocked, "{fault}");
        assert_eq!(task.checkpoint_id, None, "{fault}");
        assert_eq!(task.decision_call_id, None, "{fault}");
        assert!(
            output.result.content.starts_with("The task is blocked"),
            "{fault}: {}",
            output.result.content
        );
        assert_eq!(fixture.rig.calls().len(), 0, "{fault}: nothing is saved");
    }
    Ok(())
}

#[tokio::test]
async fn a_final_capture_saves_nothing_for_an_ending_without_history() -> Result<()> {
    let mut fixture = Fixture::new(|_| None)?;
    let root = tempfile::tempdir()?;
    let (_, sandbox) = sandbox(root.path(), Parking::QuiescenceUnproved).await?;
    let mut failed =
        crate::run_loop::failed(af_wire::report::FailureClass::RunnerCrash, "the turn broke");

    fixture
        .run()?
        .capture_final(sandbox.as_ref(), Some(&owner()), &mut failed)
        .await;

    assert!(matches!(
        failed,
        Ending::Failed {
            detail: "the turn broke",
            ..
        }
    ));
    assert_eq!(fixture.rig.calls().len(), 0);
    Ok(())
}

#[tokio::test]
async fn history_without_a_durable_task_report_still_commits_through_the_api_only_owner()
-> Result<()> {
    let mut fixture = Fixture::new(|_| None)?;
    let server = serving(&mut fixture).await?;
    let mut ending = ending(&fixture)?;
    if let Ending::Ran { output, .. } = &mut ending {
        output.task = None;
    }

    fixture.run()?.capture_without_sandbox(&mut ending).await;

    let output = report(&ending)?;
    assert!(output.recovery.is_none(), "the saved history is released");
    assert!(output.task.is_none(), "no task report is invented");
    assert_eq!(output.result.content, "done");
    assert!(
        server
            .archive
            .lock()
            .map_err(|_poisoned| error::config("archive poisoned"))?
            .calls
            .iter()
            .any(|(path, _)| path.ends_with(CHECKPOINT)),
        "the checkpoint was committed"
    );
    server.stop();
    Ok(())
}

#[tokio::test]
async fn repository_history_without_a_sandbox_is_dropped_and_the_answer_says_why() -> Result<()> {
    let mut fixture = Fixture::new(|_| None)?;
    pull_request(&mut fixture, &"a".repeat(40))?;
    let mut ending = ending(&fixture)?;
    if let Ending::Ran { output, .. } = &mut ending {
        output.task = None;
    }

    fixture.run()?.capture_without_sandbox(&mut ending).await;

    let output = report(&ending)?;
    assert!(output.recovery.is_none(), "unsaved history is released");
    assert!(output.task.is_none());
    assert!(
        output.result.content.starts_with("The task is blocked"),
        "{}",
        output.result.content
    );
    assert_eq!(fixture.rig.calls().len(), 0, "nothing reaches the owner");
    Ok(())
}
