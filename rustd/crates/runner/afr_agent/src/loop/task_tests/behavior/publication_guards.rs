//! Publication guard regressions execute the root loop and its real hosted tools.
use super::publication_fixture::{Publisher, publisher};
use super::*;
use afr_tools::sandbox::{ScriptedExecutor, ScriptedProcess};
use std::sync::Arc;

pub(super) async fn execute(
    script: &Script,
    native: &dyn LeaseVerbs,
    inputs: TaskInputsResponse<'static>,
    executor: &ScriptedExecutor,
) -> crate::Result<crate::RunOutput> {
    let (transport, _) = af_egress::testing::RecordingTransport::replying(200, "");
    let engine = crate::Loop::new(
        afr_tools::Catalog::hosted(Arc::new(transport)),
        script.replay(),
    );
    let lease = assigned_lease(&[afr_tools::catalog::EXEC_COMMAND.name()], 16_384);
    let prepared = PreparedTask {
        execution_deadline: inputs.execution_deadline,
        inputs,
        allow_actions: true,
    };
    engine
        .run(AgentRun {
            lease: &lease,
            prepared_task: Some(&prepared),
            recovered_task: None,
            memory: afr_memory::Seed::default(),
            executor: &afr_executor::ExecutorSlot::ready(executor),
            mint: &af_egress::testing::CountingMint::never(),
            verbs: native,
            checkpoint: &crate::testing::Discard,
            context_checkpoints: &crate::testing::ContextMemory::default(),
            events: &|_| {},
            meter: &Meter::default(),
            stop: &tokio_util::sync::CancellationToken::new(),
        })
        .await
}

pub(super) fn publication_call() -> af_providers::Chunk {
    call(
        "publish",
        "propose_change",
        serde_json::json!({"validation":["make","test"],"timeout_seconds":30}),
    )
}

pub(super) fn refusal(
    native: &Publisher,
    output: crate::RunOutput,
    expected: &str,
) -> crate::Result<()> {
    assert_eq!(
        native.published.lock().expect("publication requests").len(),
        0
    );
    assert_eq!(
        output
            .task
            .ok_or_else(crate::decision::unavailable)?
            .effect_watermark,
        0
    );
    let requests = native.goals.requests.lock().expect("goal requests");
    assert_eq!(requests.len(), 1);
    assert!(
        requests
            .first()
            .ok_or_else(crate::decision::unavailable)?
            .evidence
            .to_string()
            .contains(expected)
    );
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn open_command_is_stopped_before_publication_and_requires_fresh_validation()
-> crate::Result<()> {
    let native = publisher();
    let script = Script::new([vec![
        call(
            "background",
            "exec_command",
            serde_json::json!({"cmd":"tail -f build.log","yield_time_ms":250}),
        ),
        publication_call(),
        ended(false),
    ]]);
    let executor = ScriptedExecutor::new([ScriptedProcess::stays_open("ready")]);
    let output = execute(&script, &native, native.goals.inputs.clone(), &executor).await?;
    assert_eq!(executor.spawned().len(), 1);
    assert_eq!(executor.killed(), [afr_executor::ProcessId::new(1)]);
    refusal(&native, output, "Open commands were stopped")?;
    Ok(())
}

#[tokio::test]
async fn malformed_native_repository_identity_refuses_before_candidate_capture() -> crate::Result<()>
{
    let mut native = publisher();
    if let Some(task) = native.goals.inputs.current_task.as_mut()
        && let TaskTarget::ExistingPr { repository, .. } = &mut task.target
    {
        "missing-owner".clone_into(repository);
    }
    let script = Script::new([vec![publication_call(), ended(false)]]);
    let executor = ScriptedExecutor::new([]);
    let output = execute(&script, &native, native.goals.inputs.clone(), &executor).await?;
    assert_eq!(executor.spawned().len(), 0);
    refusal(&native, output, "Native repository identity is invalid")?;
    Ok(())
}
