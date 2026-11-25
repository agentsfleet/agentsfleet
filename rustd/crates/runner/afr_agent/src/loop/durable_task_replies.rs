//! Original-source reply deliveries do not depend on which run read the input.

use super::Harness;
use af_wire::durable_task::{
    DecisionDurableTaskVersion, DurableTaskInputMode, DurableTaskInputReceiptInput,
};

impl Harness<'_, '_> {
    pub(super) async fn answer_durable_task_inputs(&mut self) -> crate::Result<()> {
        let Some(prepared) = self.task.as_mut() else {
            return Ok(());
        };
        let verbs = self.shared.run.verbs;
        let sources: Vec<_> = prepared
            .inputs
            .current_interpretation
            .iter()
            .cloned()
            .chain(
                prepared
                    .inputs
                    .inputs
                    .iter()
                    .filter_map(|input| input.accepted.clone()),
            )
            .collect();
        let mut changed = false;
        for accepted in sources {
            for item in &accepted.items {
                if item.response_text.is_some() {
                    continue;
                }
                let binding = &item.binding;
                let (mode, response_text) = if !binding.applies_here {
                    (DurableTaskInputMode::Queued, None)
                } else if binding
                    .input_sequence
                    .is_none_or(|sequence| sequence <= self.incorporated_input_sequence)
                {
                    (DurableTaskInputMode::Answered, Some("Your request has been accepted into the task's ongoing work. The final task status is recorded after completion and recovery checks.".to_owned()))
                } else {
                    continue;
                };
                let task = reply_durable_task(&prepared.inputs, binding)?;
                let saved = verbs
                    .durable_task_input_receipt(DurableTaskInputReceiptInput {
                        fencing_token: self.shared.fencing_token,
                        operation_id: crate::decision::identity()?,
                        source_turn_id: accepted.source_turn_id.clone(),
                        plan_digest: accepted.plan_digest.clone(),
                        item_id: binding.item_id.clone(),
                        durable_task: task,
                        mode,
                        response_text,
                    })
                    .await
                    .map_err(crate::decision::native)?;
                if saved.item_id != binding.item_id || saved.mode != mode {
                    return Err(crate::decision::unavailable());
                }
                changed = true;
            }
        }
        if changed {
            prepared.inputs = verbs
                .durable_task_inputs(af_wire::durable_task::DurableTaskInputsQuery {
                    fencing_token: self.shared.fencing_token,
                    starting_after: None,
                    after_input_sequence: Some(self.incorporated_input_sequence),
                    limit: 4,
                })
                .await
                .map_err(crate::decision::native)?;
            prepared.execution_deadline = prepared.inputs.execution_deadline;
            if let Some(task) = prepared.inputs.current_durable_task.as_mut() {
                task.covered_input_sequence = self.incorporated_input_sequence;
                task.confirmed_effect_watermark = self.confirmed_effect_watermark;
            }
        }
        Ok(())
    }

    /// A source whose items bound several tasks keeps each queued answer
    /// beside the active task's final report, in the source's order.
    pub(super) fn combined_durable_task_answer(&self, answer: String) -> String {
        let Some(source) = self
            .task
            .as_ref()
            .and_then(|task| task.inputs.current_interpretation.as_ref())
        else {
            return answer;
        };
        let mut included = false;
        let mut parts = Vec::new();
        for item in &source.items {
            let active = item.binding.applies_here;
            if active {
                if !included {
                    parts.push(answer.as_str());
                    included = true;
                }
            } else {
                parts.extend(item.response_text.as_deref());
            }
        }
        if !included {
            parts.push(&answer);
        }
        parts.join("\n\n")
    }
}

fn reply_durable_task(
    inputs: &af_wire::durable_task::DurableTaskInputsResponse<'_>,
    binding: &af_wire::durable_task::DurableTaskItemBinding,
) -> crate::Result<Option<DecisionDurableTaskVersion>> {
    binding
        .durable_task_id
        .as_ref()
        .map(|id| {
            let generation = inputs
                .current_durable_task
                .as_ref()
                .filter(|task| task.durable_task_id == *id)
                .map(|task| task.generation)
                .or_else(|| {
                    inputs
                        .durable_tasks
                        .iter()
                        .find(|task| task.durable_task_id == *id)
                        .map(|task| task.generation)
                })
                .ok_or_else(crate::decision::unavailable)?;
            Ok(DecisionDurableTaskVersion {
                durable_task_id: id.clone(),
                generation,
            })
        })
        .transpose()
}
