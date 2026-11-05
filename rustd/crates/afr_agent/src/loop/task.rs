//! Only the root assesses completed action groups; children cannot settle goals.

use afd_wire::task::{TaskDisposition, TaskInputsQuery, TaskInputMode, TaskInputReceiptInput};
use super::{Ending, Harness};

impl Harness<'_, '_> {
    pub(super) async fn assess_task(&mut self, answer: &str) -> Option<Ending> {
        if self.depth != 0 { return None; }
        self.task.as_ref().filter(|prepared| prepared.allow_actions)?;
        let Some(assignment) = self.shared.decision else { return Some(Ending::Stopped); };
        // Tool results are data. Native report preparation independently reads
        // exact-head validation and refuses stale completion evidence.
        let evidence = match self.task_evidence(answer).await {
            Ok(evidence) => evidence,
            Err(failure) => {
                self.task_report = None;
                tracing::warn!(event = "task_evidence_unavailable", error_code = %failure.code());
                return Some(Ending::Stopped);
            }
        };
        let prepared = self.task.as_ref()?;
        let assessed = tokio::select! {
            biased;
            () = self.stop.cancelled() => return Some(Ending::Stopped),
            assessed = crate::decision::goal::assess(prepared, assignment, self.shared.lease.verbs,
                self.shared.fencing_token, evidence) => assessed,
        };
        match assessed {
            Ok(report) => {
                let disposition = report.disposition;
                self.task_report = Some(report);
                match disposition {
                    TaskDisposition::Continue if self.shared.started.elapsed() >= std::time::Duration::from_secs(10 * 60) =>
                        Some(Ending::Answered("The task remains in progress. Its state will be saved before the next execution slice.".to_owned())),
                    TaskDisposition::Continue => None,
                    TaskDisposition::Complete | TaskDisposition::Waiting | TaskDisposition::Blocked =>
                        Some(Ending::Answered(if answer.trim().is_empty() { match disposition {
                            TaskDisposition::Complete => "The action group finished and the goal assessment requested completion. Native publication and validation checks will confirm the final task status.",
                            TaskDisposition::Waiting => "The action group finished. The task is waiting for its recorded dependency to complete.",
                            _ => "The task is blocked; its current progress will be saved.",
                        }.to_owned() } else { answer.to_owned() })),
                }
            }
            Err(failure) => {
                self.task_report = None;
                tracing::warn!(event = "task_goal_unavailable", error_code = %failure.code());
                Some(Ending::Stopped)
            }
        }
    }

    pub(super) async fn incorporate_task_inputs(&mut self) -> crate::Result<()> {
        let Some(prepared) = self.task.as_mut().filter(|prepared| prepared.allow_actions) else { return Ok(()); };
        let verbs = self.shared.lease.verbs;
        // Always read the oldest unsettled source page. Advancing the message
        // cursor before all mixed-source replies are acknowledged can strand
        // an accepted conversation item while a later action completes.
        let mut inputs = verbs.task_inputs(TaskInputsQuery { fencing_token: self.shared.fencing_token,
            starting_after: None, after_input_sequence: Some(self.incorporated_input_sequence), limit: 4 })
            .await.map_err(crate::decision::native)?;
        let pending: Vec<_> = inputs.inputs.iter().filter(|input| input.accepted.is_none()).cloned().collect();
        for input in pending {
            crate::decision::Intake { lease: self.shared.payload, verbs, provider: self.shared.provider,
                meter: self.shared.meter, stop: &self.stop }
                .bind(&inputs, &input.source_admission_id, &input.message).await?;
            // Each atomic plan can advance the native generation. Never classify
            // another queued message against the previous generation's authority.
            inputs = verbs.task_inputs(TaskInputsQuery { fencing_token: self.shared.fencing_token,
                starting_after: None, after_input_sequence: Some(self.incorporated_input_sequence), limit: 4 })
                .await.map_err(crate::decision::native)?;
        }
        // A saved conversation can precede several native application receipts.
        // Read every immutable page before permitting another action; never jump
        // the local cursor to the latest native cursor without incorporation.
        for page in 0..64 {
            for input in &inputs.accepted_inputs {
            if input.sequence != self.incorporated_input_sequence.saturating_add(1) {
                return Err(crate::decision::unavailable());
            }
            let text = serde_json::json!({ "accepted_task_input": input });
            self.messages.push(afr_providers::Message::User(self.shared.scrub.clean(text.to_string()).into_inner()));
            // Durable application follows incorporation. A replacement reconstructs
            // already applied meaning from the same immutable native receipt.
            if !input.applied {
                let receipt = verbs.task_input_receipt(TaskInputReceiptInput {
                    fencing_token: self.shared.fencing_token, operation_id: crate::decision::identity()?,
                    source_admission_id: input.source_admission_id.clone(), plan_digest: input.plan_digest.clone(),
                    item_id: input.item_id.clone(), task: Some(input.task.clone()), mode: TaskInputMode::ApplyHere,
                    response_text: None,
                }).await.map_err(crate::decision::native)?;
                if receipt.item_id != input.item_id || receipt.covered_input_sequence != Some(input.sequence) {
                    return Err(crate::decision::unavailable());
                }
            }
            self.incorporated_input_sequence = input.sequence;
            if input.response_text.is_none() {
                // This acknowledges incorporation, not a semantic completion.
                // The ordinary report independently settles the final task
                // outcome after capture and current native evidence checks.
                let receipt = verbs.task_input_receipt(TaskInputReceiptInput {
                    fencing_token: self.shared.fencing_token, operation_id: crate::decision::identity()?,
                    source_admission_id: input.source_admission_id.clone(), plan_digest: input.plan_digest.clone(),
                    item_id: input.item_id.clone(), task: Some(input.task.clone()), mode: TaskInputMode::Answered,
                    response_text: Some("Your input has been incorporated into the task's ongoing work. Its final status will follow the native completion and recovery checks.".to_owned()),
                }).await.map_err(crate::decision::native)?;
                if receipt.item_id != input.item_id || receipt.mode != TaskInputMode::Answered {
                    return Err(crate::decision::unavailable());
                }
            }
        }
            let complete = inputs.current_task.as_ref().is_none_or(|task| self.incorporated_input_sequence >= task.input_sequence);
            if complete { break; }
            if inputs.accepted_inputs.is_empty() || page == 63 { return Err(crate::decision::unavailable()); }
            inputs = verbs.task_inputs(TaskInputsQuery { fencing_token: self.shared.fencing_token,
                starting_after: None, after_input_sequence: Some(self.incorporated_input_sequence), limit: 4 })
                .await.map_err(crate::decision::native)?;
        }
        prepared.inputs = inputs;
        if let Some(task) = prepared.inputs.current_task.as_mut() {
            if self.incorporated_input_sequence < task.input_sequence { return Err(crate::decision::unavailable()); }
            task.covered_input_sequence = self.incorporated_input_sequence;
            task.confirmed_effect_watermark = self.confirmed_effect_watermark;
        }
        Ok(())
    }
}

