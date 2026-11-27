//! A run's end: the sessions its calls left open, the result the report
//! carries, and what the run leaves behind.

use crate::{Completed, ExecutionResult, Failure, ResultOutcome};
use guffbolt::agents::Pause;

use super::{Ending, Harness};
use crate::engine::RunOutput;

/// What a run stopped by its run reports as its detail.
pub(super) const DETAIL_STOPPED: &str = "the run was stopped before it finished";
/// The event a run's end logs when it killed sessions still open.
pub(super) const EVENT_SESSIONS_INTERRUPTED: &str = "sessions_interrupted";

impl Harness<'_, '_> {
    async fn finish_durable_task_replies(&mut self) -> crate::Result<()> {
        let pending = self.task.as_ref().is_some_and(|task| {
            task.inputs
                .current_interpretation
                .iter()
                .chain(
                    task.inputs
                        .inputs
                        .iter()
                        .filter_map(|input| input.accepted.as_ref()),
                )
                .any(|source| source.items.iter().any(|item| item.response_text.is_none()))
        });
        if !pending {
            return Ok(());
        }
        // A run bound to no task has no execution ceiling: the fleet's budget
        // and a person's Stop bound its replies, as they bound the run. A
        // bound task whose ceiling is missing still answers nothing.
        if self.task.as_ref().is_some_and(|task| {
            task.execution_deadline.is_none() && task.inputs.current_durable_task.is_none()
        }) {
            return self.answer_durable_task_inputs().await;
        }
        let budget = self.durable_task_execution_budget();
        super::durable_task_slice::within_budget(budget, self.answer_durable_task_inputs()).await
    }

    /// Adds the answer a run ended on to its history. The loop remembers an
    /// answer the model gave, so only an ending an extension made up, such as
    /// a task's final status, is still to be added.
    fn record_answer(&mut self, ending: &Ending) {
        let Ending::Answered(text) = ending else {
            return;
        };
        let remembered = matches!(
            self.transcript.messages().last(),
            Some(guffbolt::Message::Assistant { calls, .. }) if calls.is_empty()
        );
        if !remembered {
            self.transcript.push(guffbolt::Message::Assistant {
                text: self.shared.scrub.text(text).into_owned(),
                calls: Vec::new(),
                replay: guffbolt::Replay::default(),
            });
        }
    }

    /// Closes every session the run's calls left open, then hands back the
    /// outcome, the answer and the tokens spent, with the calls and the memory
    /// the run leaves for the supervisor to post. The root's end; a child
    /// concludes instead.
    pub(super) async fn finish(mut self, mut ending: Ending, pause: Option<&Pause>) -> RunOutput {
        self.close_sessions().await;
        if matches!(ending, Ending::Answered(_))
            && let Err(failure) = self.finish_durable_task_replies().await
        {
            tracing::warn!(event = "task_input_reply_pending", error_code = %failure.code());
            self.durable_task_report = None;
            if let Ending::Answered(text) = &mut ending {
                text.push_str("\n\nThe task paused because its input replies could not be acknowledged. Its final status has not been confirmed.");
            }
        }
        if let Ending::Answered(text) = ending {
            ending = Ending::Answered(self.combined_durable_task_answer(text));
        }
        self.record_answer(&ending);
        self.settle_saves(&mut ending, pause).await;
        let recovery = self.final_recovery(&mut ending, pause).await;
        let folded_through_sequence = self
            .folded_through()
            .filter(|_| matches!(ending, Ending::Answered(_)));
        let shared = self.shared;
        let (outcome, content) = match ending {
            Ending::Answered(text) => (
                ResultOutcome::Completed(Completed),
                shared.scrub.text(&text).into_owned(),
            ),
            Ending::Failed(failure) => {
                let failed = Failure {
                    class: failure.failure_class(),
                    detail: failure.detail().into(),
                };
                (ResultOutcome::Failed(failed), String::new())
            }
            Ending::Stopped => {
                let stopped = Failure {
                    class: None,
                    detail: DETAIL_STOPPED.into(),
                };
                (ResultOutcome::Failed(stopped), String::new())
            }
            Ending::Waiting(waiting) => (
                ResultOutcome::Waiting,
                shared.scrub.text(&waiting.question).into_owned(),
            ),
        };
        let usage = shared.meter.read();
        let (trace, records) = shared.ledger.finish();
        let memory = shared.run.memory.pending().await;
        RunOutput {
            recovery,
            task: self.durable_task_report,
            result: ExecutionResult {
                outcome,
                content: content.into(),
                token_count: usage.total(),
                wall_seconds: shared.started.elapsed().as_secs(),
                input_tokens: usage.input,
                cached_input_tokens: usage.cached_input,
                output_tokens: usage.output,
            },
            memory,
            trace: Some(trace),
            records,
            folded_through_sequence,
        }
    }

    /// Saves a waiting run's question, and settles only after the last save's
    /// commit, so nothing the run reports names a generation still publishing.
    async fn settle_saves(&mut self, ending: &mut Ending, pause: Option<&Pause>) {
        self.settle_publication(ending).await;
        if matches!(ending, Ending::Waiting(_)) {
            self.save_waiting(pause).await;
            self.settle_publication(ending).await;
        }
    }

    async fn final_recovery(
        &mut self,
        ending: &mut Ending,
        pause: Option<&Pause>,
    ) -> Option<crate::DurableTaskRecovery> {
        let recovery_durable_task = self
            .task
            .as_ref()
            .and_then(|task| task.inputs.current_durable_task.clone());
        match recovery_durable_task {
            None => None,
            Some(task) => match self.capture_durable_task_root(task, pause).await {
                Ok(recovery) => Some(recovery),
                Err(failure) => {
                    tracing::warn!(event = "task_history_capture_failed", error_code = %failure.code());
                    self.durable_task_report = None;
                    if matches!(*ending, Ending::Answered(_)) {
                        *ending = Ending::Answered("The task is blocked because its private history could not be saved. No continuation or completion was authorized.".to_owned());
                    }
                    None
                }
            },
        }
    }

    /// Kills every process a session still holds, so none outlives the run
    /// whatever becomes of its sandbox, and logs how many there were. A call
    /// the run stopped mid-yield left its process registered, so it is
    /// closed here too.
    async fn close_sessions(&self) {
        let Some(agent) = self.shared.agent.peek() else {
            return;
        };
        let sessions = self
            .shared
            .run
            .sessions
            .close_all(&afr_tools::SandboxEnv::new(agent))
            .await;
        if sessions > 0 {
            let run_id = self.shared.run_id;
            let event = EVENT_SESSIONS_INTERRUPTED;
            tracing::info!(run_id, sessions, event);
        }
    }
}
