//! How a task root's answer, action group and native input cursor decide
//! whether the root ends, and with what.

use super::*;

fn prepared(native: &Goals, allow_actions: bool) -> PreparedDurableTask {
    PreparedDurableTask {
        execution_deadline: native.inputs.execution_deadline,
        inputs: native.inputs.clone(),
        allow_actions,
    }
}

#[tokio::test]
async fn a_root_without_action_authority_answers_without_a_goal_assessment() -> crate::Result<()> {
    let script = Script::new([vec![say("Here is what I found."), ended(false)]]);
    let engine = super::super::super::tests::engine(vec![], &script);
    let this_run = assigned_run(&[], 16_384);
    let native = goals([]);

    let output = run(&engine, &this_run, &native, &prepared(&native, false)).await?;

    assert_eq!(output.result.content, "Here is what I found.");
    assert!(output.task.is_none(), "no assessment, no task report");
    assert!(native.requests.lock().is_ok_and(|sent| sent.is_empty()));
    Ok(())
}

#[tokio::test]
async fn a_completed_action_group_with_no_answer_reports_the_completion_it_was_granted()
-> crate::Result<()> {
    let tool = &afr_tools::catalog::UPDATE_PLAN;
    let script = Script::new([vec![
        call("plan", tool.name(), serde_json::json!({})),
        ended(false),
    ]]);
    let engine =
        super::super::super::tests::engine(vec![Canned::boxed(tool, "Plan updated")], &script);
    let this_run = assigned_run(&[tool.name()], 16_384);
    let native = goals([evaluation("DONE", Some(1.0), 1.0)]);

    let output = run(&engine, &this_run, &native, &prepared(&native, true)).await?;

    let report = output.task.ok_or_else(crate::decision::unavailable)?;
    assert_eq!(report.disposition, DurableTaskDisposition::Complete);
    assert!(
        output
            .result
            .content
            .starts_with("The action group finished and the goal assessment requested completion."),
        "{}",
        output.result.content
    );
    assert_eq!(script.sent().len(), 1, "the group ended the root");
    Ok(())
}

#[tokio::test]
async fn a_native_input_cursor_ahead_of_every_readable_page_stops_the_root_before_a_model_call()
-> crate::Result<()> {
    let script = Script::new([]);
    let engine = super::super::super::tests::engine(vec![], &script);
    let this_run = assigned_run(&[], 16_384);
    let mut native = goals([]);
    if let Some(task) = native.inputs.current_durable_task.as_mut() {
        task.input_sequence = 1;
    }

    let output = run(&engine, &this_run, &native, &prepared(&native, true)).await?;

    assert!(script.sent().is_empty(), "the model was called");
    assert!(output.task.is_none());
    assert_eq!(output.result.content, "");
    Ok(())
}

#[tokio::test]
async fn inputs_naming_no_current_task_are_incorporated_but_stop_the_root_before_acting()
-> crate::Result<()> {
    let script = Script::new([vec![say("Investigation recorded."), ended(false)]]);
    let engine = super::super::super::tests::engine(vec![], &script);
    let this_run = assigned_run(&[], 16_384);
    let mut native = goals([evaluation("WAIT", Some(1.0), 0.0)]);
    let prepared = prepared(&native, true);
    native.inputs.current_durable_task = None;

    let output = run(&engine, &this_run, &native, &prepared).await?;

    // Incorporation has no task cursor to advance; effect reconciliation
    // then has no task to read effects for, so nothing may act.
    assert!(script.sent().is_empty(), "the model was called");
    assert!(
        output.task.is_none(),
        "a task the native inputs no longer name has nothing to report"
    );
    Ok(())
}
