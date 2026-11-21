//! A completed group goes to its goal assessment whole; only a refusal of its
//! size buys a summary that retains whole-group provenance.

#![expect(
    clippy::expect_used,
    reason = "test fixture: poisoned state must fail the test"
)]

use super::*;

const SUMMARY: &str = "The investigation produced evidence; validation is still pending.";

/// A decision model that reads `bytes` of state and questions whole.
fn reading(bytes: u32, native: &mut Goals, this_run: &mut af_wire::run::RunPayload<'_>) {
    native.input_max_bytes = usize::try_from(bytes).expect("fixture limit fits usize");
    if let Some(decision) = this_run.decision.as_mut() {
        decision.profile.input_max_bytes = bytes;
    }
}

#[tokio::test]
async fn raw_evidence_within_the_models_limit_is_sent_whole() -> crate::Result<()> {
    let tool = &afr_tools::catalog::UPDATE_PLAN;
    let script = Script::new([vec![
        call("plan", tool.name(), serde_json::json!({})),
        ended(false),
    ]]);
    let engine = super::super::super::tests::engine(
        vec![Canned::boxed(tool, &"evidence ".repeat(1400))],
        &script,
    );
    let this_run = assigned_run(&[tool.name()], 65_536);
    let native = goals([evaluation("WAIT", Some(1.0), 0.0)]);
    let prepared = PreparedTask {
        execution_deadline: native.inputs.execution_deadline,
        inputs: native.inputs.clone(),
        allow_actions: true,
    };
    let output = run(&engine, &this_run, &native, &prepared).await?;
    assert_eq!(
        output
            .task
            .ok_or_else(crate::decision::unavailable)?
            .disposition,
        TaskDisposition::Waiting
    );
    // Past the retired 8 KiB threshold, within the fixture model's 24 KiB.
    let whole = canonical(1400);
    assert!(whole.len() > 8 * 1024);
    let requests = native.requests.lock().expect("fixture mutex is unpoisoned");
    assert_eq!(requests.len(), 1);
    let sent = requests.first().ok_or_else(crate::decision::unavailable)?;
    assert_eq!(
        sent.evidence,
        serde_json::from_str::<serde_json::Value>(&whole)?
    );
    assert_eq!(sent.retry_of, None);
    // No summarising model call followed the action group.
    assert_eq!(script.sent().len(), 1);
    assert!(output.recovery.is_some());
    Ok(())
}

#[tokio::test]
async fn evidence_past_the_models_limit_is_summarised_and_retried() -> crate::Result<()> {
    let tool = &afr_tools::catalog::UPDATE_PLAN;
    let script = Script::new([
        vec![
            call("plan", tool.name(), serde_json::json!({})),
            ended(false),
        ],
        vec![say(SUMMARY), ended(false)],
    ]);
    let engine = super::super::super::tests::engine(
        vec![Canned::boxed(tool, &"evidence ".repeat(1400))],
        &script,
    );
    let mut this_run = assigned_run(&[tool.name()], 65_536);
    let mut native = goals([evaluation("WAIT", Some(1.0), 0.0)]);
    reading(8192, &mut native, &mut this_run);
    let prepared = PreparedTask {
        execution_deadline: native.inputs.execution_deadline,
        inputs: native.inputs.clone(),
        allow_actions: true,
    };
    let output = run(&engine, &this_run, &native, &prepared).await?;
    assert_eq!(
        output
            .task
            .ok_or_else(crate::decision::unavailable)?
            .disposition,
        TaskDisposition::Waiting
    );
    let requests = native.requests.lock().expect("fixture mutex is unpoisoned");
    // One refused whole group, then one summary.
    assert_eq!(requests.len(), 2);
    let mut requests = requests.iter();
    let whole = requests.next().ok_or_else(crate::decision::unavailable)?;
    let summary = requests.next().ok_or_else(crate::decision::unavailable)?;
    assert_eq!(
        whole.evidence,
        serde_json::from_str::<serde_json::Value>(&canonical(1400))?
    );
    assert_complete_summary(&summary.evidence, 1400);
    // A refused request reserved no physical call for `retry_of` to name.
    assert_eq!((&whole.retry_of, &summary.retry_of), (&None, &None));
    assert_ne!(whole.operation_id, summary.operation_id);
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
async fn a_group_past_16_kib_within_the_models_limit_is_sent_whole() -> crate::Result<()> {
    let tool = &afr_tools::catalog::UPDATE_PLAN;
    let script = Script::new([
        vec![
            call("plan", tool.name(), serde_json::json!({})),
            ended(false),
        ],
        vec![say(SUMMARY), ended(false)],
    ]);
    let engine = super::super::super::tests::engine(
        vec![Canned::boxed(tool, &"evidence ".repeat(2000))],
        &script,
    );
    let this_run = assigned_run(&[tool.name()], 65_536);
    let native = goals([evaluation("WAIT", Some(1.0), 0.0)]);
    let prepared = PreparedTask {
        execution_deadline: native.inputs.execution_deadline,
        inputs: native.inputs.clone(),
        allow_actions: true,
    };
    let output = run(&engine, &this_run, &native, &prepared).await?;
    assert_eq!(
        output
            .task
            .ok_or_else(crate::decision::unavailable)?
            .disposition,
        TaskDisposition::Waiting
    );
    assert!(canonical(2000).len() > 16 * 1024, "past the old wire bound");
    let requests = native.requests.lock().expect("fixture mutex is unpoisoned");
    // The whole group is sent once, and no summary is asked for.
    assert_eq!(requests.len(), 1);
    let whole = requests.first().ok_or_else(crate::decision::unavailable)?;
    assert_eq!(whole.evidence.to_string(), canonical(2000));
    assert_eq!(script.sent().len(), 1, "no summarising model call");
    Ok(())
}

#[tokio::test]
async fn empty_oversized_cut_or_tool_bearing_summary_blocks_goal_assessment() -> crate::Result<()> {
    let tool = &afr_tools::catalog::UPDATE_PLAN;
    // The whole group is refused as too large first; an oversize summary is
    // sent and refused too, and every other summary never leaves the runner.
    for (summary, requests) in [
        (vec![say("  "), ended(false)], 1),
        (vec![say(&"x".repeat(9000)), ended(false)], 2),
        (vec![say("partial evidence"), ended(true)], 1),
        (
            vec![
                call("not-allowed", tool.name(), serde_json::json!({})),
                ended(false),
            ],
            1,
        ),
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
        let mut this_run = assigned_run(&[tool.name()], 65_536);
        let mut native = goals([]);
        reading(8192, &mut native, &mut this_run);
        let prepared = PreparedTask {
            execution_deadline: native.inputs.execution_deadline,
            inputs: native.inputs.clone(),
            allow_actions: true,
        };
        let output = run(&engine, &this_run, &native, &prepared).await?;
        assert!(output.task.is_none());
        assert!(output.recovery.is_some());
        assert_eq!(
            output.result.outcome,
            crate::ResultOutcome::Failed(crate::Failure {
                class: None,
                detail: "the run was stopped before it finished".into(),
            })
        );
        assert_eq!(
            native
                .requests
                .lock()
                .expect("fixture mutex is unpoisoned")
                .len(),
            requests
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
    let this_run = assigned_run(&[], 16_384);
    let mut native = goals([evaluation("CONTINUE", Some(1.0), 0.0)]);
    native.inputs.execution_deadline = Some(af_core::clock::after(
        af_core::clock::now(),
        std::time::Duration::from_secs(150),
    ));
    let prepared = PreparedTask {
        execution_deadline: native.inputs.execution_deadline,
        inputs: native.inputs.clone(),
        allow_actions: true,
    };
    let output = run(&engine, &this_run, &native, &prepared).await?;
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

/// Independent literal bytes of this fixture's complete two-message group.
/// Do not call the production evidence builder: dropping or reordering a member must fail.
fn canonical(repeats: usize) -> String {
    [
        r#"{"answer":"","completed_group":[{"role":"assistant","text":"","calls":[{"name":"update_plan","arguments":{}}]},{"role":"tool","call_id":"plan","output":""#,
        &"evidence ".repeat(repeats),
        r#""}]}"#,
    ].concat()
}

fn assert_complete_summary(evidence: &serde_json::Value, repeats: usize) {
    assert_eq!(
        evidence.get("summary").and_then(serde_json::Value::as_str),
        Some(SUMMARY)
    );
    assert_eq!(
        evidence.get("summary_is_semantic_evidence"),
        Some(&serde_json::json!(true))
    );
    let canonical = canonical(repeats);
    assert_eq!(
        evidence["complete_group_bytes"],
        serde_json::json!(canonical.len())
    );
    assert_eq!(
        evidence["complete_group_sha256"],
        serde_json::json!(af_core::etag::sha256_hex(canonical.as_bytes()))
    );
    assert_eq!(
        evidence
            .get("complete_group_sha256")
            .and_then(serde_json::Value::as_str)
            .map(str::len),
        Some(64)
    );
}
