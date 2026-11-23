//! Accepted source replies preserve ordering and require native acknowledgement.

#![expect(
    clippy::expect_used,
    reason = "test fixture: poisoned state must fail the test"
)]

use super::*;

#[derive(Debug)]
pub(super) struct Replies {
    pub(super) goals: Goals,
    pub(super) inputs: Mutex<DurableTaskInputsResponse<'static>>,
    pub(super) receipts: Mutex<Vec<DurableTaskInputReceiptInput>>,
    pub(super) wrong_ack: bool,
}

#[async_trait::async_trait]
impl RunVerbs for Replies {
    async fn schedules(&self, _: ScheduleCall<'_>) -> Result<String, Unanswered> {
        Err(Unanswered::Unreachable)
    }
    async fn message(&self, _: u64, _: &str) -> Result<bool, Unanswered> {
        Err(Unanswered::Unreachable)
    }
    async fn durable_task_inputs(
        &self,
        _: DurableTaskInputsQuery,
    ) -> Result<DurableTaskInputsResponse<'static>, Unanswered> {
        Ok(self
            .inputs
            .lock()
            .expect("fixture mutex is unpoisoned")
            .clone())
    }
    async fn durable_task_effects(
        &self,
        _: DurableTaskEffectsQuery,
    ) -> Result<DurableTaskEffectsPage, Unanswered> {
        Ok(DurableTaskEffectsPage {
            effects: vec![],
            next_cursor: None,
        })
    }
    async fn durable_task_decision(
        &self,
        input: DecisionReservationInput,
    ) -> Result<DecisionEvaluation, Unanswered> {
        self.goals.durable_task_decision(input).await
    }
    async fn durable_task_input_receipt(
        &self,
        input: DurableTaskInputReceiptInput,
    ) -> Result<DurableTaskInputReceiptResult, Unanswered> {
        assert_eq!(input.source_turn_id, SOURCE);
        assert_eq!(input.mode, DurableTaskInputMode::Answered);
        let result = DurableTaskInputReceiptResult {
            item_id: if self.wrong_ack {
                SOURCE.to_owned()
            } else {
                input.item_id.clone()
            },
            mode: input.mode,
            covered_input_sequence: None,
            replayed: false,
        };
        let mut inputs = self.inputs.lock().expect("fixture mutex is unpoisoned");
        if !self.wrong_ack
            && let Some(source) = inputs.current_interpretation.as_mut()
        {
            for item in &mut source.items {
                if item.binding.item_id == input.item_id {
                    item.response_text.clone_from(&input.response_text);
                }
            }
        }
        self.receipts
            .lock()
            .expect("fixture mutex is unpoisoned")
            .push(input);
        Ok(result)
    }
}

pub(super) fn source(items: Vec<DurableTaskRecoveredItem>) -> Replies {
    let goals = goals([evaluation("WAIT", Some(1.0), 0.0)]);
    let mut input = goals.inputs.clone();
    let message = items
        .iter()
        .map(|item| item.accepted_text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    input.current_message = Some(message.clone());
    input.current_interpretation = Some(DurableTaskAcceptedInterpretation {
        source_turn_id: SOURCE.to_owned(),
        plan_digest: "b".repeat(64),
        message,
        items,
    });
    Replies {
        goals,
        inputs: Mutex::new(input),
        receipts: Mutex::new(vec![]),
        wrong_ack: false,
    }
}

pub(super) fn item(
    id: &str,
    action: DurableTaskInputAction,
    applies_here: bool,
) -> DurableTaskRecoveredItem {
    let text = match action {
        DurableTaskInputAction::Conversation => "hello",
        DurableTaskInputAction::Status => "what is the investigation status?",
        _ => "investigate the failure",
    };
    DurableTaskRecoveredItem {
        binding: DurableTaskItemBinding {
            item_id: id.to_owned(),
            durable_task_id: if action == DurableTaskInputAction::Conversation {
                None
            } else {
                Some(SOURCE.to_owned())
            },
            durable_task_generation: if action == DurableTaskInputAction::Conversation {
                None
            } else {
                Some(1)
            },
            action,
            applies_here,
            input_sequence: None,
        },
        accepted_text: text.to_owned(),
        interpretation: "Preserve the source request".to_owned(),
        response_text: None,
    }
}

pub(super) async fn execute(
    script: &Script,
    native: &Replies,
    allow_actions: bool,
) -> crate::Result<crate::RunOutput> {
    let engine = super::super::super::tests::engine(vec![], script);
    let this_run = assigned_run(&[], 16_384);
    let inputs = native
        .inputs
        .lock()
        .expect("fixture mutex is unpoisoned")
        .clone();
    let prepared = PreparedDurableTask {
        execution_deadline: inputs.execution_deadline,
        inputs,
        allow_actions,
    };
    run(&engine, &this_run, native, &prepared).await
}
