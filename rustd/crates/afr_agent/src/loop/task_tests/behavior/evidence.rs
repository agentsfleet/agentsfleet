//! Large completed groups retain whole-group provenance through bounded summaries.

#![expect(
    clippy::expect_used,
    reason = "test fixture: poisoned state must fail the test"
)]

use super::*;

#[tokio::test]
async fn oversized_group_is_summarized_without_losing_its_complete_digest() -> crate::Result<()> {
    let tool = &afr_tools::catalog::UPDATE_PLAN;
    let script = Script::new([
        vec![
            call("plan", tool.name(), serde_json::json!({})),
            ended(false),
        ],
        vec![
            say("The investigation produced evidence; validation is still pending."),
            ended(false),
        ],
    ]);
    let engine = super::super::super::tests::engine(
        vec![Canned::boxed(tool, &"evidence ".repeat(1400))],
        &script,
    );
    let lease = assigned_lease(&[tool.name()], 65_536);
    let native = goals([evaluation("WAIT", Some(1.0), 0.0)]);
    let prepared = PreparedTask {
        execution_deadline: native.inputs.execution_deadline,
        inputs: native.inputs.clone(),
        allow_actions: true,
    };
    let output = run(&engine, &lease, &native, &prepared).await?;
    assert_eq!(
        output
            .task
            .ok_or_else(crate::decision::unavailable)?
            .disposition,
        TaskDisposition::Waiting
    );
    let requests = native.requests.lock().expect("fixture mutex is unpoisoned");
    let evidence = &requests
        .first()
        .ok_or_else(crate::decision::unavailable)?
        .evidence;
    assert_complete_summary(evidence);
    let sent = script.sent();
    assert_eq!(sent.len(), 2);
    assert!(sent.last().is_some_and(|request| {
        request.tools.is_empty()
            && request
                .instructions
                .contains("Do not declare the task complete")
    }));
    assert!(output.recovery.is_some());
    Ok(())
}

#[tokio::test]
async fn empty_oversized_cut_or_tool_bearing_summary_blocks_goal_assessment() -> crate::Result<()> {
    let tool = &afr_tools::catalog::UPDATE_PLAN;
    for summary in [
        vec![say("  "), ended(false)],
        vec![say(&"x".repeat(9000)), ended(false)],
        vec![say("partial evidence"), ended(true)],
        vec![
            call("not-allowed", tool.name(), serde_json::json!({})),
            ended(false),
        ],
    ] {
        let script = Script::new([
            vec![
                call("plan", tool.name(), serde_json::json!({})),
                ended(false),
            ],
            summary,
        ]);
        let engine = super::super::super::tests::engine(
            vec![Canned::boxed(tool, &"evidence ".repeat(1400))],
            &script,
        );
        let lease = assigned_lease(&[tool.name()], 65_536);
        let native = goals([]);
        let prepared = PreparedTask {
            execution_deadline: native.inputs.execution_deadline,
            inputs: native.inputs.clone(),
            allow_actions: true,
        };
        let output = run(&engine, &lease, &native, &prepared).await?;
        assert!(output.task.is_none());
        assert!(output.recovery.is_some());
        assert!(matches!(
            output.result.outcome,
            crate::ResultOutcome::Failed(_)
        ));
        assert!(
            native
                .requests
                .lock()
                .expect("fixture mutex is unpoisoned")
                .is_empty()
        );
        assert_eq!(script.sent().len(), 2);
    }
    Ok(())
}

#[tokio::test]
async fn expiring_slice_assesses_continuation_before_starting_another_action() -> crate::Result<()>
{
    let script = Script::new([]);
    let engine = super::super::super::tests::engine(vec![], &script);
    let lease = assigned_lease(&[], 16_384);
    let mut native = goals([evaluation("CONTINUE", Some(1.0), 0.0)]);
    native.inputs.execution_deadline = Some(afd_core::clock::after(
        afd_core::clock::now(),
        std::time::Duration::from_secs(150),
    ));
    let prepared = PreparedTask {
        execution_deadline: native.inputs.execution_deadline,
        inputs: native.inputs.clone(),
        allow_actions: true,
    };
    let output = run(&engine, &lease, &native, &prepared).await?;
    assert_eq!(
        output
            .task
            .ok_or_else(crate::decision::unavailable)?
            .disposition,
        TaskDisposition::Continue
    );
    assert_eq!(
        output.result.content,
        "The task remains in progress. Its state will be saved before the next execution slice."
    );
    assert!(script.sent().is_empty());
    assert!(output.recovery.is_some());
    assert_eq!(
        native
            .requests
            .lock()
            .expect("fixture mutex is unpoisoned")
            .len(),
        1
    );
    Ok(())
}

fn assert_complete_summary(evidence: &serde_json::Value) {
    assert_eq!(
        evidence.get("summary").and_then(serde_json::Value::as_str),
        Some("The investigation produced evidence; validation is still pending.")
    );
    assert_eq!(
        evidence.get("summary_is_semantic_evidence"),
        Some(&serde_json::json!(true))
    );
    assert!(
        evidence
            .get("complete_group_bytes")
            .and_then(serde_json::Value::as_u64)
            .is_some_and(|bytes| bytes > 8192)
    );
    assert_eq!(
        evidence
            .get("complete_group_sha256")
            .and_then(serde_json::Value::as_str)
            .map(str::len),
        Some(64)
    );
}
