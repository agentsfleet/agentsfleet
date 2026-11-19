//! The turn itself: the engine runs, beside its sandbox's build when the run
//! has one, and reaches the sandbox's executor through the build's slot.

use std::panic::AssertUnwindSafe;

use af_core::error_code;
use af_core::timing::ENGINE_STOP_GRACE;
use af_wire::report::FailureClass;
use afr_agent::AgentRun;
use afr_executor::ExecutorSource;
use afr_memory::Seed;
use futures_util::FutureExt as _;

use super::{DETAIL_RENEWAL, Run, failed};
use crate::activity::ActivitySink;
use crate::credentials::RunMint;
use crate::memory::RunCheckpoint;
use crate::report::Ending;
use crate::verbs::FencedVerbs;

const DETAIL_ENGINE: &str = "the agent engine stopped before the turn ended";
const DETAIL_PANIC: &str = "the agent engine panicked";
const EVENT_ENGINE_FAILED: &str = "engine_run_failed";
const EVENT_ENGINE_PANICKED: &str = "engine_panicked";

pub(super) struct Recovered<'a> {
    pub(super) task: Option<&'a afr_agent::TaskRecovery>,
    pub(super) turn: Option<&'a afr_agent::TurnRecovery>,
}

impl<'a> From<Option<&'a super::recovery::State>> for Recovered<'a> {
    fn from(state: Option<&'a super::recovery::State>) -> Self {
        Self {
            task: state.and_then(|state| state.history.as_ref()),
            turn: state.and_then(|state| state.turn.as_ref()),
        }
    }
}

/// An inbox that claims nothing.
///
/// A durable task reads what people send it as task inputs, and the daemon's
/// fold claim (`claim_inputs.sql`) refuses a run that carries a task, so
/// asking it would cost a round trip for an empty answer.
#[derive(Debug)]
struct Closed;

#[async_trait::async_trait]
impl afr_agent::Inbox for Closed {
    async fn claim(&self, _after_sequence: i64) -> Vec<afr_agent::Folded> {
        Vec::new()
    }
}

/// Where a run claims the messages its fleet received while it works: nowhere
/// for a durable task, through the run's own fence for every other run.
fn inbox_of<'a>(
    task: Option<&afr_agent::PreparedTask>,
    run: &'a dyn afr_agent::Inbox,
) -> &'a dyn afr_agent::Inbox {
    if task.is_some() { &Closed } else { run }
}

impl Run<'_> {
    /// Runs the turn. A panicking engine is caught here, inside a sandbox's
    /// lifetime when there is one, so the caller still destroys it.
    pub(super) async fn drive(
        &self,
        memory: Seed<'_>,
        executor: &dyn ExecutorSource,
        sink: ActivitySink,
        prepared_task: Option<&afr_agent::PreparedTask>,
        recovered: Recovered<'_>,
        recovery: &dyn afr_agent::RecoveryBoundarySink,
    ) -> Ending {
        let mint = RunMint::new(&self.lessee.plane, &self.ids.run);
        let checkpoint = RunCheckpoint::new(&self.lessee.plane, &self.ids.fleet, self.payload);
        let verbs = FencedVerbs::new(
            &self.lessee.plane,
            &self.ids.run,
            self.payload.fencing_token,
            &self.lessee.halt,
            &self.interrupt,
            &self.decisions,
        );
        let run = AssertUnwindSafe(self.lessee.agent.run(AgentRun {
            recovery: Some(recovery),
            recovered_task: recovered.task,
            recovered_turn: recovered.turn,
            prepared_task,
            run: self.payload,
            memory,
            executor,
            mint: &mint,
            verbs: &verbs,
            checkpoint: &checkpoint,
            inbox: inbox_of(prepared_task, &verbs),
            events: &sink,
            meter: &self.meter,
            stop: &self.interrupt,
        }))
        .catch_unwind();
        let abandoned = async {
            self.interrupt.cancelled().await;
            tokio::time::sleep(ENGINE_STOP_GRACE).await;
        };
        sink.run_started();
        let output = tokio::select! {
            biased;
            output = run => Some(output),
            () = abandoned => None,
        };
        let timing = sink.timing();
        let first_chunk = timing.first_chunk;
        self.timeline.ran(timing);
        drop(sink);
        self.ended(output, first_chunk)
    }

    fn ended(
        &self,
        output: Option<std::thread::Result<afr_agent::Result<afr_agent::RunOutput>>>,
        first_chunk: Option<std::time::Duration>,
    ) -> Ending {
        match output {
            Some(Ok(Ok(output))) => Ending::Ran {
                output: Box::new(output),
                first_chunk,
            },
            Some(Ok(Err(failure))) => self.fail(
                &failure,
                FailureClass::RunnerCrash,
                EVENT_ENGINE_FAILED,
                DETAIL_ENGINE,
            ),
            Some(Err(_panic)) => {
                let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
                let run_id = self.ids.run.as_str();
                let event = EVENT_ENGINE_PANICKED;
                {
                    let (error_code, message) = (
                        &code,
                        format_args!("the agent engine panicked; a sandbox is still torn down"),
                    );
                    tracing::error!(message, error_code, run_id, event);
                };
                failed(FailureClass::RunnerCrash, DETAIL_PANIC)
            }
            None => failed(FailureClass::RenewalTerminate, DETAIL_RENEWAL),
        }
    }
}

#[cfg(test)]
mod tests {
    use af_wire::task::TaskInputsResponse;
    use afr_agent::{Folded, Inbox, PreparedTask};

    use super::inbox_of;

    /// An inbox with one message waiting.
    #[derive(Debug)]
    struct Waiting;

    #[async_trait::async_trait]
    impl Inbox for Waiting {
        async fn claim(&self, _after_sequence: i64) -> Vec<Folded> {
            vec![Folded {
                event_id: "019f0000-0000-7000-8000-000000000001".to_owned(),
                sequence: 1,
                text: "use staging".to_owned(),
            }]
        }
    }

    fn task() -> PreparedTask {
        PreparedTask {
            execution_deadline: None,
            inputs: TaskInputsResponse {
                source_admission_id: "admission".to_owned(),
                accepted_revision_id: None,
                current_message: None,
                inputs: Vec::new(),
                accepted_inputs: Vec::new(),
                current_task: None,
                execution_deadline: None,
                current_interpretation: None,
                tasks: Vec::new(),
                next_cursor: None,
            },
            allow_actions: false,
        }
    }

    #[tokio::test]
    async fn a_durable_task_claims_no_folded_message() {
        let run = Waiting;
        assert_eq!(
            inbox_of(Some(&task()), &run).claim(0).await,
            Vec::<Folded>::new()
        );
    }

    #[tokio::test]
    async fn every_other_run_claims_through_its_run() {
        let run = Waiting;
        assert_eq!(inbox_of(None, &run).claim(0).await.len(), 1);
    }
}
