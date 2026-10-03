//! The turn itself: the engine runs, inside a sandbox's lifetime when the lease
//! has one.

use std::panic::AssertUnwindSafe;

use afd_core::error_code;
use afd_wire::memory::MemoryDelta;
use afd_wire::report::FailureClass;
use afr_agent::AgentRun;
use afr_executor::Executor;
use futures_util::FutureExt as _;

use super::{DETAIL_RENEWAL, LeaseRun, failed};
use crate::activity::ActivitySink;
use crate::credentials::LeaseMint;
use crate::memory::LeaseCheckpoint;
use crate::report::Ending;

const DETAIL_ENGINE: &str = "the agent engine stopped before the turn ended";
const DETAIL_PANIC: &str = "the agent engine panicked";
const EVENT_ENGINE_FAILED: &str = "engine_run_failed";
const EVENT_ENGINE_PANICKED: &str = "engine_panicked";

impl LeaseRun<'_> {
    /// Runs the turn. A panicking engine is caught here, inside a sandbox's
    /// lifetime when there is one, so the caller still destroys it.
    pub(super) async fn drive(
        &self,
        memory: &[MemoryDelta<'_>],
        executor: Option<&dyn Executor>,
        sink: ActivitySink,
    ) -> Ending {
        let mint = LeaseMint::new(&self.lessee.plane, &self.ids.lease);
        let checkpoint = LeaseCheckpoint::new(&self.lessee.plane, &self.ids.fleet, self.lease);
        let run = AssertUnwindSafe(self.lessee.agent.run(AgentRun {
            lease: self.lease,
            memory,
            executor,
            mint: &mint,
            checkpoint: &checkpoint,
            events: &sink,
            stop: &self.interrupt,
        }))
        .catch_unwind();
        let output = tokio::select! {
            output = run => Some(output),
            () = self.interrupt.cancelled() => None,
        };
        let first_chunk = sink.first_chunk();
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
