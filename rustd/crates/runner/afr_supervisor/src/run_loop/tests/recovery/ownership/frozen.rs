//! A held sandbox's frozen files and history commit as one checkpoint, and a
//! capture that cannot prove its owner, workspace or tenant blocks the task.

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

const fn owner() -> State {
    State {
        history: None,
        turn: None,
        base: None,
        base_paths: Vec::new(),
    }
}

#[tokio::test]
async fn a_frozen_capture_commits_history_and_names_its_checkpoint_in_the_report() -> Result<()> {
    let mut fixture = Fixture::new(|_| None)?;
    let mut server = Server::start(Refusal::None).await;
    let plane = server
        .plane
        .take()
        .ok_or_else(|| error::config("fixture plane missing"))?;
    Arc::get_mut(&mut fixture.rig.lessee)
        .ok_or_else(|| error::config("fixture lessee is shared"))?
        .plane = plane;
    let root = tempfile::tempdir()?;
    let (engine, sandbox) = sandbox(root.path(), Freezer::Works).await?;
    let mut ending = ending(&fixture)?;

    fixture
        .run()?
        .capture_frozen(sandbox.as_ref(), Some(&owner()), &mut ending)
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
        engine.frozen.load(Ordering::SeqCst),
        0,
        "capture reads the sandbox its holder froze"
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
async fn a_frozen_capture_it_cannot_prove_blocks_the_durable_task_without_native_io() -> Result<()>
{
    for fault in ["owner", "workspace", "toolbox", "unproved", "busy"] {
        let mut fixture = Fixture::new(|_| None)?;
        let root = tempfile::tempdir()?;
        let engine = FakeEngine {
            workspace: (fault != "workspace").then(|| root.path().to_owned()),
            toolbox: (fault != "toolbox").then(api_toolbox),
            freezer: match fault {
                "unproved" => Freezer::QuiescenceUnproved,
                "busy" => Freezer::BusyTenant,
                _ => Freezer::Works,
            },
            ..FakeEngine::default()
        };
        let sandbox = engine
            .prepare(SandboxRequest::new(test_support::RUN_ID, Limits::default()))
            .await
            .map_err(|source| error::boundary("fixture sandbox", source))?;
        let state = owner();
        let mut ending = ending(&fixture)?;

        fixture
            .run()?
            .capture_frozen(
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
async fn a_frozen_capture_saves_nothing_for_an_ending_without_history() -> Result<()> {
    let mut fixture = Fixture::new(|_| None)?;
    let root = tempfile::tempdir()?;
    let (_, sandbox) = sandbox(root.path(), Freezer::QuiescenceUnproved).await?;
    let mut failed =
        crate::run_loop::failed(af_wire::report::FailureClass::RunnerCrash, "the turn broke");

    fixture
        .run()?
        .capture_frozen(sandbox.as_ref(), Some(&owner()), &mut failed)
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
async fn a_fresh_api_durable_task_takes_no_file_permit_and_restores_nothing() -> Result<()> {
    let mut fixture = Fixture::new(|_| None)?;
    let task = fixture
        .prepared
        .inputs
        .current_durable_task
        .as_mut()
        .ok_or_else(|| error::config("fixture task missing"))?;
    task.checkpoint_id = None;
    task.checkpoint_generation = None;
    let root = tempfile::tempdir()?;
    let (engine, sandbox) = sandbox(root.path(), Freezer::Works).await?;
    let owner = OnceLock::new();
    let run = fixture.run()?;
    let build = fixture.build(&owner);

    run.prepare_recovery(sandbox.as_ref(), &build).await?;

    let state = owner
        .get()
        .ok_or_else(|| error::config("recovery owner missing"))?;
    assert!(state.history.is_none());
    assert!(state.base.is_none());
    assert_eq!(state.base_paths, Vec::<String>::new());
    assert_eq!(engine.frozen.load(Ordering::SeqCst), 1);
    assert_eq!(engine.thawed.load(Ordering::SeqCst), 1);
    let again = run.prepare_recovery(sandbox.as_ref(), &build).await;
    assert!(matches!(
        again.as_ref().map_err(crate::Error::kind),
        Err(error::ErrorKind::Config { detail }) if *detail == "sandbox recovery was prepared twice"
    ));
    assert_eq!(
        engine.thawed.load(Ordering::SeqCst),
        2,
        "a refusal still thaws"
    );
    drop(run);
    assert_eq!(fixture.rig.calls().len(), 0, "a fresh task reads nothing");
    Ok(())
}

#[tokio::test]
async fn a_durable_workspace_without_a_native_durable_task_never_freezes() -> Result<()> {
    let mut fixture = Fixture::new(|_| None)?;
    fixture.prepared.inputs.current_durable_task = None;
    let root = tempfile::tempdir()?;
    let (engine, sandbox) = sandbox(root.path(), Freezer::Works).await?;
    let owner = OnceLock::new();

    let result = fixture
        .run()?
        .prepare_recovery(sandbox.as_ref(), &fixture.build(&owner))
        .await;

    assert!(matches!(
        result.as_ref().map_err(crate::Error::kind),
        Err(error::ErrorKind::Config { detail }) if *detail == "durable workspace has no native task"
    ));
    assert!(owner.get().is_none());
    assert_eq!(engine.frozen.load(Ordering::SeqCst), 0);
    assert_eq!(fixture.rig.calls().len(), 0);
    Ok(())
}

/// A native owner whose plan retains an already committed base of one empty
/// page, the shape a fresh pull-request checkout's first capture answers.
fn retained(call: &Call) -> Option<Answer> {
    let retained = || {
        let mut plan = fixture::plan()?;
        plan.mode = CheckpointMode::RetainBase;
        Ok::<_, crate::Error>(plan)
    };
    match call.verb {
        Verb::DurableTaskCheckpointPlan => Some(match retained() {
            Ok(plan) => test_support::json(&plan),
            Err(failure) => Answer::Fail(failure),
        }),
        Verb::DurableTaskCheckpointAuthorizations => Some(match retained() {
            Ok(plan) => test_support::json(&CheckpointAuthorizations {
                plan,
                objects: vec![],
            }),
            Err(failure) => Answer::Fail(failure),
        }),
        Verb::DurableTaskCheckpointPage => Some(test_support::json(&fixture::page())),
        _ => None,
    }
}

/// The fixture's task, now a fresh pull request whose checkout heads at `head`.
fn pull_request(fixture: &mut Fixture, head: &str) -> Result<()> {
    let task = fixture
        .prepared
        .inputs
        .current_durable_task
        .as_mut()
        .ok_or_else(|| error::config("fixture task missing"))?;
    task.checkpoint_id = None;
    task.checkpoint_generation = None;
    task.target = DurableTaskTarget::ExistingPr {
        repository: "acme/widget".to_owned(),
        repository_id: 42,
        number: 3,
        branch: "repair".to_owned(),
        base: "main".to_owned(),
        head: head.to_owned(),
        tree: "b".repeat(40),
        url: "https://github.com/acme/widget/pull/3".to_owned(),
        workflows: vec![],
    };
    Ok(())
}

#[tokio::test]
async fn a_fresh_pull_request_retains_its_checkout_as_the_base_before_the_model_runs() -> Result<()>
{
    let root = tempfile::tempdir()?;
    let repository = root.path().join("widget");
    test_support::repository(&repository);
    let head = test_support::head(&repository, "HEAD");
    let mut fixture = Fixture::new(retained)?;
    pull_request(&mut fixture, &head)?;
    let (engine, sandbox) = sandbox(root.path(), Freezer::Works).await?;
    let owner = OnceLock::new();

    fixture
        .run()?
        .prepare_recovery(sandbox.as_ref(), &fixture.build(&owner))
        .await?;

    let state = owner
        .get()
        .ok_or_else(|| error::config("recovery owner missing"))?;
    assert!(state.history.is_none(), "a fresh task has no history");
    let base = state
        .base
        .as_ref()
        .ok_or_else(|| error::config("the checkout was not retained"))?;
    assert_eq!(base.checkpoint_id, ID);
    assert_eq!(base.mode, CheckpointMode::RetainBase);
    assert!(state.base_paths.is_empty(), "the retained page is empty");
    assert_eq!(engine.thawed.load(Ordering::SeqCst), 1);
    let calls = fixture.rig.calls();
    let planned = calls
        .iter()
        .find(|call| call.verb == Verb::DurableTaskCheckpointPlan)
        .and_then(|call| call.body.as_ref())
        .ok_or_else(|| error::config("no base was planned"))?;
    let planned: af_wire::durable_task::CheckpointPlanInput =
        serde_json::from_slice(planned).map_err(error::encode)?;
    assert_eq!(planned.mode, CheckpointMode::RetainBase);
    assert_eq!(planned.commit_oid.as_deref(), Some(head.as_str()));
    assert!(
        calls
            .iter()
            .any(|call| call.verb == Verb::DurableTaskCheckpointAuthorizations),
        "the retained base was inspected"
    );
    Ok(())
}

#[tokio::test]
async fn a_pull_request_checkout_that_moved_is_refused_before_any_capture() -> Result<()> {
    let root = tempfile::tempdir()?;
    let repository = root.path().join("widget");
    test_support::repository(&repository);
    let mut fixture = Fixture::new(retained)?;
    pull_request(&mut fixture, &"c".repeat(40))?;
    let (engine, sandbox) = sandbox(root.path(), Freezer::Works).await?;
    let owner = OnceLock::new();

    let result = fixture
        .run()?
        .prepare_recovery(sandbox.as_ref(), &fixture.build(&owner))
        .await;

    assert!(matches!(
        result.as_ref().map_err(crate::Error::kind),
        Err(error::ErrorKind::Config { detail }) if *detail == "task branch moved before initial capture"
    ));
    assert!(owner.get().is_none());
    assert_eq!(
        engine.thawed.load(Ordering::SeqCst),
        1,
        "a refusal still thaws"
    );
    assert_eq!(fixture.rig.calls().len(), 0, "nothing was planned");
    Ok(())
}

#[tokio::test]
async fn history_without_a_durable_task_report_still_commits_through_the_api_only_owner()
-> Result<()> {
    let mut fixture = Fixture::new(|_| None)?;
    let mut server = Server::start(Refusal::None).await;
    let plane = server
        .plane
        .take()
        .ok_or_else(|| error::config("fixture plane missing"))?;
    Arc::get_mut(&mut fixture.rig.lessee)
        .ok_or_else(|| error::config("fixture lessee is shared"))?
        .plane = plane;
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
