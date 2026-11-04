//! The turn itself: the engine runs, beside its sandbox's build when the lease
//! has one, and reaches the sandbox's executor through the build's slot.

use std::panic::AssertUnwindSafe;

use afd_core::error_code;
use afd_core::timing::ENGINE_STOP_GRACE;
use afd_wire::report::FailureClass;
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

impl LeaseRun<'_> {
    /// Runs the turn. A panicking engine is caught here, inside a sandbox's
    /// lifetime when there is one, so the caller still destroys it.
    pub(super) async fn drive(
        &self,
        memory: Seed<'_>,
        executor: &dyn ExecutorSource,
        sink: ActivitySink,
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
        );
        let run = AssertUnwindSafe(self.lessee.agent.run(AgentRun {
            lease: self.lease,
            memory,
            executor,
            mint: &mint,
            verbs: &verbs,
            checkpoint: &checkpoint,
            context_checkpoints: &context_checkpoints,
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
        match output {
            Some(Ok(Ok(output))) => Ending::Ran {
                output,
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
