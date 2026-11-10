//! The turn itself: the engine runs, beside its sandbox's build when the lease
//! has one, and reaches the sandbox's executor through the build's slot.

use std::panic::AssertUnwindSafe;

use af_core::error_code;
use af_core::timing::ENGINE_STOP_GRACE;
use af_wire::report::FailureClass;
use afr_agent::AgentRun;
use afr_executor::ExecutorSource;
use afr_memory::Seed;
use futures_util::FutureExt as _;

use super::{DETAIL_RENEWAL, LeaseRun, failed};
use crate::activity::ActivitySink;
use crate::credentials::LeaseMint;
use crate::memory::LeaseCheckpoint;
use crate::report::Ending;
use crate::verbs::FencedVerbs;

const DETAIL_ENGINE: &str = "the agent engine stopped before the turn ended";
const DETAIL_PANIC: &str = "the agent engine panicked";
const EVENT_ENGINE_FAILED: &str = "engine_run_failed";
const EVENT_ENGINE_PANICKED: &str = "engine_panicked";

/// An inbox that claims nothing.
///
/// A durable task reads what people send it as task inputs, and the daemon's
/// fold claim (`claim_inputs.sql`) refuses a lease that carries a task, so
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
/// for a durable task, through the lease's own fence for every other run.
fn inbox_of<'a>(
    task: Option<&afr_agent::PreparedTask>,
    lease: &'a dyn afr_agent::Inbox,
) -> &'a dyn afr_agent::Inbox {
    if task.is_some() { &Closed } else { lease }
}

impl LeaseRun<'_> {
    /// Runs the turn. A panicking engine is caught here, inside a sandbox's
    /// lifetime when there is one, so the caller still destroys it.
    pub(super) async fn drive(
        &self,
        memory: Seed<'_>,
        executor: &dyn ExecutorSource,
        sink: ActivitySink,
        prepared_task: Option<&afr_agent::PreparedTask>,
        recovered_task: Option<&afr_agent::TaskRecovery>,
    ) -> Ending {
        let mint = LeaseMint::new(&self.lessee.plane, &self.ids.lease);
        let checkpoint = LeaseCheckpoint::new(&self.lessee.plane, &self.ids.fleet, self.lease);
        let context_checkpoints = crate::context::LeaseContext::new(
            &self.lessee.plane,
            &self.ids.lease,
            self.lease.fencing_token,
        );
        let verbs = FencedVerbs::new(
            &self.lessee.plane,
            &self.ids.lease,
            self.lease.fencing_token,
            &self.lessee.halt,
            &self.interrupt,
            &self.decisions,
        );
        let run = AssertUnwindSafe(self.lessee.agent.run(AgentRun {
            recovered_task,
            prepared_task,
            lease: self.lease,
            memory,
            executor,
            mint: &mint,
            verbs: &verbs,
            checkpoint: &checkpoint,
            context_checkpoints: &context_checkpoints,
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
                let lease_id = self.ids.lease.as_str();
                let event = EVENT_ENGINE_PANICKED;
                tracing::error!(
                    error_code = code,
                    lease_id,
                    event,
                    "the agent engine panicked; a sandbox is still torn down"
                );
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
        let lease = Waiting;
        assert_eq!(
            inbox_of(Some(&task()), &lease).claim(0).await,
            Vec::<Folded>::new()
        );
    }

    #[tokio::test]
    async fn every_other_run_claims_through_its_lease() {
        let lease = Waiting;
        assert_eq!(inbox_of(None, &lease).claim(0).await.len(), 1);
    }
}
