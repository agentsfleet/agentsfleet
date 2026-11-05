//! Original-source reply obligations do not depend on which lease read the input.

use afd_wire::task::{TaskInputAction, TaskInputMode, TaskInputReceiptInput, DecisionTaskVersion};
use super::Harness;

impl Harness<'_, '_> {
    pub(super) async fn answer_task_inputs(&mut self) -> crate::Result<()> {
        let Some(prepared) = self.task.as_mut() else { return Ok(()); };
        let intake = crate::decision::Intake { lease: self.shared.payload, verbs: self.shared.lease.verbs,
            provider: self.shared.provider, meter: self.shared.meter, stop: &self.stop };
        let sources: Vec<_> = prepared.inputs.current_interpretation.iter().cloned()
            .chain(prepared.inputs.inputs.iter().filter_map(|input| input.accepted.clone())).collect();
        let mut changed = false;
        for accepted in sources {
            for item in &accepted.items {
                if item.response_text.is_some() { continue; }
                let binding = &item.binding;
                let (mode, response_text) = if matches!(binding.action, TaskInputAction::Conversation | TaskInputAction::Status | TaskInputAction::Clarify) {
                    (TaskInputMode::Answered, Some(intake.reply(&accepted, item, &prepared.inputs).await?))
                } else if !binding.applies_here {
                    (TaskInputMode::Queued, None)
                } else if binding.input_sequence.is_none_or(|sequence| sequence <= self.incorporated_input_sequence) {
                    (TaskInputMode::Answered, Some("Your request has been accepted into the task's ongoing work. The final task status is recorded after completion and recovery checks.".to_owned()))
                } else { continue; };
                let task = binding.task_id.as_ref().map(|id| {
                    let generation = prepared.inputs.current_task.as_ref().filter(|task| task.task_id == *id)
                        .map(|task| task.generation)
                        .or_else(|| prepared.inputs.tasks.iter().find(|task| task.task_id == *id).map(|task| task.generation))
                        .ok_or_else(crate::decision::unavailable)?;
                    Ok::<_, crate::Error>(DecisionTaskVersion { task_id: id.clone(), generation })
                }).transpose()?;
                let saved = intake.verbs.task_input_receipt(TaskInputReceiptInput {
                    fencing_token: self.shared.fencing_token, operation_id: crate::decision::identity()?,
                    source_admission_id: accepted.source_admission_id.clone(), plan_digest: accepted.plan_digest.clone(),
                    item_id: binding.item_id.clone(), task, mode, response_text,
                }).await.map_err(crate::decision::native)?;
                if saved.item_id != binding.item_id || saved.mode != mode { return Err(crate::decision::unavailable()); }
                changed = true;
            }
        }
        if changed {
            prepared.inputs = intake.verbs.task_inputs(afd_wire::task::TaskInputsQuery {
                fencing_token: self.shared.fencing_token, starting_after: None,
                after_input_sequence: Some(self.incorporated_input_sequence), limit: 4,
            }).await.map_err(crate::decision::native)?;
            if let Some(task) = prepared.inputs.current_task.as_mut() {
                task.covered_input_sequence = self.incorporated_input_sequence;
                task.confirmed_effect_watermark = self.confirmed_effect_watermark;
            }
        }
        Ok(())
    }

    pub(super) fn accepted_answer(&self) -> Option<String> {
        let prepared = self.task.as_ref().filter(|prepared| !prepared.allow_actions)?;
        if prepared.inputs.current_message.is_some() && prepared.inputs.current_interpretation.is_none() {
            return Some("I couldn't establish a reliable interpretation of this request, so I haven't started any work. Please clarify which task or action you want.".to_owned());
        }
        let source = prepared.inputs.current_interpretation.as_ref()?;
        if source.items.is_empty() { return None; }
        let answers: Option<Vec<_>> = source.items.iter().map(|item| item.response_text.as_deref()).collect();
        answers.map(|answers| answers.join("\n\n"))
    }

    /// A mixed source keeps its conversational and queued answers beside the
    /// active task's final report, in the original interpretation order.
    pub(super) fn combined_task_answer(&self, answer: String) -> String {
        let Some(source) = self.task.as_ref().filter(|task| task.allow_actions)
            .and_then(|task| task.inputs.current_interpretation.as_ref()) else { return answer; };
        let mut included = false;
        let mut parts = Vec::new();
        for item in &source.items {
            let active = item.binding.applies_here && matches!(item.binding.action,
                TaskInputAction::NewWork | TaskInputAction::Refresh | TaskInputAction::Steer);
            if active {
                if !included { parts.push(answer.as_str()); included = true; }
            } else if let Some(reply) = item.response_text.as_deref() { parts.push(reply); }
        }
        if !included { parts.push(&answer); }
        parts.join("\n\n")
    }
}
