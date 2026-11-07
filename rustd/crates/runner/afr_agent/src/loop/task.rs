//! What a durable task adds to its root loop, and the steps it adds.
//!
//! Only the root assesses completed action groups; children cannot settle goals.

use super::{Ending, Harness};
use af_wire::task::{TaskDisposition, TaskInputMode, TaskInputReceiptInput, TaskInputsQuery};
use guffbolt::Budget;

use crate::turn::Turn;

/// What becomes of a task's answer, a failure being the provider's.
type Answer = guffbolt::Answer<af_providers::Error>;

/// What the model is told when its goal assessment says to go on.
const CONTINUE: &str = "The accepted task goal remains incomplete. Continue the next bounded action group using the current evidence.";
/// What an assessment of a recovered run's first turn is told.
const RECOVERED: &str = "Recovery reconciled; no new action has run.";
/// What the assessment that ends an execution slice is told.
const SLICE_ENDING: &str = "Execution slice is ending; preserve current work for continuation.";

/// A durable task's additions to its root loop: its inputs and confirmed
/// effects read before each model call, its goal assessed after each action
/// group, and its execution slice as the loop's budget. The loop itself
/// (`guffbolt::drive`) knows none of this.
pub(super) struct Task;

impl<'s, 'run> guffbolt::Extension<Harness<'s, 'run>> for Task {
    fn budget(&self, root: &Harness<'s, 'run>) -> Budget {
        if !root.task.as_ref().is_some_and(|task| task.allow_actions) {
            return Budget::Unbounded;
        }
        root.task_execution_budget()
            .map_or(Budget::Spent, Budget::Within)
    }

    fn budget_spent(&mut self, root: &mut Harness<'s, 'run>) {
        // An interrupted group never carries a prior group's completion authority.
        root.task_report = None;
    }

    async fn before_model(&mut self, root: &mut Harness<'s, 'run>, number: u64) -> Option<Ending> {
        if root.incorporate_task_inputs().await.is_err()
            || root.reconcile_task_effects().await.is_err()
            || root.answer_task_inputs().await.is_err()
        {
            return Some(Ending::Stopped);
        }
        if let Some(answer) = root.accepted_answer() {
            return Some(Ending::Answered(answer));
        }
        if number == 1
            && root.shared.recovered_task.is_some()
            && let Some(ending) = root.assess_task(RECOVERED).await
        {
            return Some(ending);
        }
        if root.task_yield_due() {
            return Some(
                root.assess_task(SLICE_ENDING)
                    .await
                    .unwrap_or(Ending::Stopped),
            );
        }
        None
    }

    fn after_turn(&mut self, _root: &Harness<'s, 'run>, turn: &Turn) -> Option<Ending> {
        // A task never acts on a turn its stream did not finish, or one cut at
        // its output limit.
        (!turn.finished || turn.cut).then_some(Ending::Stopped)
    }

    async fn on_answer(&mut self, root: &mut Harness<'s, 'run>, answer: &str) -> Answer {
        if let Some(ending) = root.assess_task(answer).await {
            return Answer::End(ending);
        }
        if root
            .task_report
            .as_ref()
            .is_some_and(|report| report.disposition == TaskDisposition::Continue)
        {
            return Answer::Continue(CONTINUE.to_owned());
        }
        Answer::Accept
    }

    async fn after_group(&mut self, root: &mut Harness<'s, 'run>) -> Option<Ending> {
        if root.incorporate_task_inputs().await.is_err()
            || root.reconcile_task_effects().await.is_err()
        {
            return Some(Ending::Stopped);
        }
        root.assess_task("").await
    }
}

impl Harness<'_, '_> {
    pub(super) async fn assess_task(&mut self, answer: &str) -> Option<Ending> {
        self.task
            .as_ref()
            .filter(|prepared| prepared.allow_actions)?;
        let Some(assignment) = self.shared.decision else {
            return Some(Ending::Stopped);
        };
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
                    TaskDisposition::Continue if self.task_yield_due() =>
                        Some(Ending::Answered("The task remains in progress. Its state will be saved before the next execution slice.".to_owned())),
                    TaskDisposition::Continue => None,
                    TaskDisposition::Complete | TaskDisposition::Waiting | TaskDisposition::Blocked =>
                        Some(Ending::Answered(if answer.trim().is_empty() { match disposition {
                            TaskDisposition::Complete => "The action group finished and the goal assessment requested completion. Native publication and validation checks will confirm the final task status.",
                            TaskDisposition::Waiting => "The action group finished. The task is waiting for its recorded dependency to complete.",
                            TaskDisposition::Blocked | TaskDisposition::Continue => "The task is blocked; its current progress will be saved.",
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
        let Some(prepared) = self.task.as_mut().filter(|prepared| prepared.allow_actions) else {
            return Ok(());
        };
        let verbs = self.shared.lease.verbs;
        // Always read the oldest unsettled source page. Advancing the message
        // cursor before all mixed-source replies are acknowledged can strand
        // an accepted conversation item while a later action completes.
        let mut inputs = read_inputs(
            verbs,
            self.shared.fencing_token,
            self.incorporated_input_sequence,
        )
        .await?;
        let pending: Vec<_> = inputs
            .inputs
            .iter()
            .filter(|input| input.accepted.is_none())
            .cloned()
            .collect();
        for input in pending {
            let binding = crate::decision::Intake {
                lease: self.shared.payload,
                verbs,
                provider: self.shared.provider,
                meter: self.shared.meter,
                stop: &self.stop,
            }
            .bind(&inputs, &input.source_admission_id, &input.message)
            .await?;
            prepared.execution_deadline =
                match (prepared.execution_deadline, binding.execution_deadline) {
                    (Some(previous), Some(current)) => Some(previous.min(current)),
                    _ => None,
                };
            // Each atomic plan can advance the native generation. Never classify
            // another queued message against the previous generation's authority.
            inputs = read_inputs(
                verbs,
                self.shared.fencing_token,
                self.incorporated_input_sequence,
            )
            .await?;
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
                self.messages.push(af_providers::Message::User(
                    self.shared.scrub.clean(text.to_string()).into_inner(),
                ));
                // Durable application follows incorporation. A replacement reconstructs
                // already applied meaning from the same immutable native receipt.
                if !input.applied {
                    record_input(
                        verbs,
                        self.shared.fencing_token,
                        input,
                        TaskInputMode::ApplyHere,
                        None,
                    )
                    .await?;
                }
                self.incorporated_input_sequence = input.sequence;
                if input.response_text.is_none() {
                    // This acknowledges incorporation, not a semantic completion.
                    // The ordinary report independently settles the final task
                    // outcome after capture and current native evidence checks.
                    record_input(verbs, self.shared.fencing_token, input, TaskInputMode::Answered,
                        Some("Your input has been incorporated into the task's ongoing work. Its final status will follow the native completion and recovery checks.")).await?;
                }
            }
            let complete = inputs
                .current_task
                .as_ref()
                .is_none_or(|task| self.incorporated_input_sequence >= task.input_sequence);
            if complete {
                break;
            }
            if inputs.accepted_inputs.is_empty() || page == 63 {
                return Err(crate::decision::unavailable());
            }
            inputs = read_inputs(
                verbs,
                self.shared.fencing_token,
                self.incorporated_input_sequence,
            )
            .await?;
        }
        prepared.execution_deadline = inputs.execution_deadline;
        prepared.inputs = inputs;
        if let Some(task) = prepared.inputs.current_task.as_mut() {
            if self.incorporated_input_sequence < task.input_sequence {
                return Err(crate::decision::unavailable());
            }
            task.covered_input_sequence = self.incorporated_input_sequence;
            task.confirmed_effect_watermark = self.confirmed_effect_watermark;
        }
        Ok(())
    }
}

async fn record_input(
    verbs: &dyn afr_tools::LeaseVerbs,
    fencing_token: u64,
    input: &af_wire::task::TaskAcceptedInput,
    mode: TaskInputMode,
    response: Option<&str>,
) -> crate::Result<()> {
    let receipt = verbs
        .task_input_receipt(TaskInputReceiptInput {
            fencing_token,
            operation_id: crate::decision::identity()?,
            source_admission_id: input.source_admission_id.clone(),
            plan_digest: input.plan_digest.clone(),
            item_id: input.item_id.clone(),
            task: Some(input.task.clone()),
            mode,
            response_text: response.map(str::to_owned),
        })
        .await
        .map_err(crate::decision::native)?;
    if receipt.item_id != input.item_id
        || receipt.mode != mode
        || mode == TaskInputMode::ApplyHere
            && receipt.covered_input_sequence != Some(input.sequence)
    {
        return Err(crate::decision::unavailable());
    }
    Ok(())
}

async fn read_inputs(
    verbs: &dyn afr_tools::LeaseVerbs,
    fencing_token: u64,
    incorporated: u64,
) -> crate::Result<af_wire::task::TaskInputsResponse<'static>> {
    verbs
        .task_inputs(TaskInputsQuery {
            fencing_token,
            starting_after: None,
            after_input_sequence: Some(incorporated),
            limit: 4,
        })
        .await
        .map_err(crate::decision::native)
}
