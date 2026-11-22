//! Immutable input and effect receipts are reconciled before root actions resume.

#![expect(
    clippy::expect_used,
    reason = "test fixture: poisoned state must fail the test"
)]

use super::*;

#[derive(Debug)]
pub(super) struct Receipts {
    pub(super) goals: Goals,
    pub(super) inputs: Vec<DurableTaskAcceptedInput>,
    pub(super) recorded: Mutex<Vec<DurableTaskInputReceiptInput>>,
    pub(super) effects: Vec<DurableTaskEffect>,
    pub(super) resolved: Option<DurableTaskEffect>,
    pub(super) reads: Mutex<Vec<String>>,
    pub(super) wrong_receipt: bool,
}

#[async_trait::async_trait]
impl RunVerbs for Receipts {
    async fn schedules(&self, _: ScheduleCall<'_>) -> Result<String, Unanswered> {
        Err(Unanswered::Unreachable)
    }
    async fn message(&self, _: u64, _: &str) -> Result<bool, Unanswered> {
        Err(Unanswered::Unreachable)
    }
    async fn durable_task_inputs(
        &self,
        query: DurableTaskInputsQuery,
    ) -> Result<DurableTaskInputsResponse<'static>, Unanswered> {
        assert!(query.starting_after.is_none());
        let mut input = self.goals.inputs.clone();
        input.accepted_inputs = self
            .inputs
            .iter()
            .filter(|item| item.sequence > query.after_input_sequence.unwrap_or(0))
            .take(1)
            .cloned()
            .collect();
        let applied = self
            .recorded
            .lock()
            .expect("fixture mutex is unpoisoned")
            .iter()
            .filter(|receipt| receipt.mode == DurableTaskInputMode::ApplyHere)
            .count();
        let covered = u64::try_from(applied).expect("a receipt count fits a u64");
        if let Some(task) = input.current_durable_task.as_mut() {
            task.input_sequence = self.inputs.last().map_or(0, |item| item.sequence);
            task.covered_input_sequence = self
                .inputs
                .iter()
                .filter(|item| item.applied)
                .map(|item| item.sequence)
                .max()
                .unwrap_or(covered)
                .max(covered);
            task.effect_cursor = self.effects.last().map_or(0, |effect| effect.ordinal);
            task.confirmed_effect_watermark = self
                .resolved
                .as_ref()
                .filter(|effect| effect.state == DurableTaskEffectState::Confirmed)
                .map_or_else(
                    || {
                        self.effects
                            .iter()
                            .filter_map(|effect| {
                                (effect.state == DurableTaskEffectState::Confirmed)
                                    .then_some(effect.ordinal)
                            })
                            .max()
                            .unwrap_or(0)
                    },
                    |effect| effect.ordinal,
                );
        }
        Ok(input)
    }
    async fn durable_task_input_receipt(
        &self,
        input: DurableTaskInputReceiptInput,
    ) -> Result<DurableTaskInputReceiptResult, Unanswered> {
        let sequence = self
            .inputs
            .iter()
            .find(|item| item.item_id == input.item_id)
            .map(|item| item.sequence);
        let result = DurableTaskInputReceiptResult {
            item_id: if self.wrong_receipt {
                SOURCE.to_owned()
            } else {
                input.item_id.clone()
            },
            mode: input.mode,
            covered_input_sequence: sequence,
            replayed: false,
        };
        self.recorded
            .lock()
            .expect("fixture mutex is unpoisoned")
            .push(input);
        Ok(result)
    }
    async fn durable_task_effects(
        &self,
        query: DurableTaskEffectsQuery,
    ) -> Result<DurableTaskEffectsPage, Unanswered> {
        Ok(DurableTaskEffectsPage {
            effects: self
                .effects
                .iter()
                .filter(|effect| effect.ordinal > query.ordinal_after)
                .cloned()
                .collect(),
            next_cursor: None,
        })
    }
    async fn durable_task_effect(&self, effect_id: &str) -> Result<DurableTaskEffect, Unanswered> {
        self.reads
            .lock()
            .expect("fixture mutex is unpoisoned")
            .push(effect_id.to_owned());
        self.resolved.clone().ok_or(Unanswered::Unreachable)
    }
    async fn durable_task_decision(
        &self,
        input: DecisionReservationInput,
    ) -> Result<DecisionEvaluation, Unanswered> {
        self.goals.durable_task_decision(input).await
    }
}

pub(super) fn receipts() -> Receipts {
    Receipts {
        goals: goals([evaluation("WAIT", Some(1.0), 0.0)]),
        inputs: vec![],
        recorded: Mutex::new(vec![]),
        effects: vec![],
        resolved: None,
        reads: Mutex::new(vec![]),
        wrong_receipt: false,
    }
}

pub(super) fn accepted(item_id: &str, sequence: u64) -> DurableTaskAcceptedInput {
    DurableTaskAcceptedInput {
        source_turn_id: PENDING.to_owned(),
        plan_digest: "f".repeat(64),
        item_id: item_id.to_owned(),
        durable_task: DecisionDurableTaskVersion {
            durable_task_id: SOURCE.to_owned(),
            generation: 1,
        },
        sequence,
        action: DurableTaskInputAction::Steer,
        original_message: "Preserve the existing tests".to_owned(),
        accepted_text: "Preserve the existing tests".to_owned(),
        interpretation: "Do not remove tests when repairing".to_owned(),
        applied: false,
        response_text: None,
    }
}

pub(super) async fn execute(
    script: &Script,
    native: &Receipts,
    recovered: Option<&crate::DurableTaskRecovery>,
) -> crate::Result<crate::RunOutput> {
    let engine = super::super::super::tests::engine(vec![], script);
    let this_run = assigned_run(&[], 16_384);
    let prepared = PreparedDurableTask {
        execution_deadline: native.goals.inputs.execution_deadline,
        inputs: native.goals.inputs.clone(),
        allow_actions: true,
    };
    engine
        .run(AgentRun {
            recovery: None,
            run: &this_run,
            prepared_durable_task: Some(&prepared),
            recovered_durable_task: recovered,
            recovered_turn: None,
            memory: afr_memory::Seed::default(),
            executor: &afr_executor::ExecutorSlot::unavailable(
                afr_executor::Unavailable::NoSandbox,
            ),
            mint: &af_egress::testing::CountingMint::never(),
            verbs: native,
            checkpoint: &crate::testing::Discard,
            inbox: &crate::testing::NoInbox,
            events: &|_| {},
            meter: &Meter::default(),
            stop: &tokio_util::sync::CancellationToken::new(),
        })
        .await
}
