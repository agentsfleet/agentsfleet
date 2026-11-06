//! Late source obligations must revoke a goal report when acknowledgement fails.
use super::*;
use std::sync::atomic::AtomicUsize;

#[derive(Debug)]
struct LateReply {
    replies: replies_fixture::Replies,
    reads: AtomicUsize,
}

#[async_trait::async_trait]
impl LeaseVerbs for LateReply {
    async fn schedules(&self, _: ScheduleCall<'_>) -> Result<String, Unanswered> {
        Err(Unanswered::Unreachable)
    }
    async fn message(&self, _: &str) -> Result<bool, Unanswered> {
        Err(Unanswered::Unreachable)
    }
    async fn task_inputs(
        &self,
        query: TaskInputsQuery,
    ) -> Result<TaskInputsResponse<'static>, Unanswered> {
        let mut page = self.replies.task_inputs(query).await?;
        // The initial input read and effect reconciliation both precede root work.
        // Deliver this source only at the completed action boundary.
        if self.reads.fetch_add(1, Ordering::SeqCst) < 2 {
            page.current_interpretation = None;
        }
        Ok(page)
    }
    async fn task_effects(&self, query: TaskEffectsQuery) -> Result<TaskEffectsPage, Unanswered> {
        self.replies.task_effects(query).await
    }
    async fn task_decision(
        &self,
        input: DecisionReservationInput,
    ) -> Result<DecisionEvaluation, Unanswered> {
        self.replies.task_decision(input).await
    }
    async fn task_input_receipt(
        &self,
        input: TaskInputReceiptInput,
    ) -> Result<TaskInputReceiptResult, Unanswered> {
        self.replies.task_input_receipt(input).await
    }
}

#[tokio::test]
async fn late_unacknowledged_reply_revokes_goal_report_and_retains_recovery() -> crate::Result<()> {
    let mut replies = replies_fixture::source(vec![replies_fixture::item(
        ITEM,
        TaskInputAction::Conversation,
        false,
    )]);
    replies.wrong_ack = true;
    let native = LateReply {
        replies,
        reads: AtomicUsize::new(0),
    };
    let tool = &afr_tools::catalog::EXEC_COMMAND;
    let script = Script::new([
        vec![
            call("inspect", tool.name(), serde_json::json!({"cmd":"true"})),
            ended(false),
        ],
        vec![say("Hello."), ended(false)],
    ]);
    let engine =
        super::super::super::tests::engine(vec![Canned::boxed(tool, "inspected")], &script);
    let lease = assigned_lease(&[tool.name()], 16_384);
    let prepared = PreparedTask {
        inputs: native.replies.goals.inputs.clone(),
        execution_deadline: native.replies.goals.inputs.execution_deadline,
        allow_actions: true,
    };
    let output = run(&engine, &lease, &native, &prepared).await?;
    assert_late_reply_outcome(&native, &script, &output);
    Ok(())
}

fn assert_late_reply_outcome(native: &LateReply, script: &Script, output: &crate::RunOutput) {
    assert_eq!(
        output.result.outcome,
        crate::ResultOutcome::Completed(crate::Completed)
    );
    assert!(output.task.is_none());
    assert!(output.recovery.is_some());
    assert!(
        output
            .result
            .content
            .contains("input replies could not be acknowledged")
    );
    assert_eq!(
        native
            .replies
            .goals
            .requests
            .lock()
            .expect("goal calls")
            .len(),
        1
    );
    let receipts = native.replies.receipts.lock().expect("source replies");
    assert_eq!(receipts.len(), 1);
    assert_eq!(
        receipts
            .first()
            .and_then(|receipt| receipt.response_text.as_deref()),
        Some("Hello.")
    );
    assert_eq!(script.sent().len(), 2);
}
