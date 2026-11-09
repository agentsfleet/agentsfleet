//! Prepare one fleet turn and run it with its memory window and executor.

use af_wire::memory::MemoryHydrateResponse;
use af_wire::report::FailureClass;
use afr_executor::{ExecutorSlot, Unavailable};
use afr_memory::Seed;

use super::{
    DETAIL_MEMORY, DETAIL_RENEWAL, DETAIL_TURN, EVENT_HYDRATE_FAILED, LeaseRun, hold::Worked,
    refusal::failed, segments::Segment,
};
use crate::activity::ActivitySink;
use crate::memory;
use crate::turns::FleetTurns;

impl LeaseRun<'_> {
    /// Everything that holds the fleet's turn: setup and the run. The sandbox
    /// the run used comes back with its ending, for the lease to hold or
    /// destroy once the cut and the report decide.
    ///
    /// The interrupt ends the run early, and the caller's ending replaces
    /// whatever this returns.
    pub(super) async fn work(&self, turns: &FleetTurns, sink: ActivitySink) -> Worked {
        let lessee = self.lessee;
        let needs = match lessee.agent.admit(&self.lease.policy) {
            Ok(needs) => needs,
            Err(refusal) => return self.unhosted(&refusal).into(),
        };
        let claimed = tokio::select! {
            turn = turns.claim(&self.ids.fleet) => turn,
            () = self.interrupt.cancelled() => {
                return failed(FailureClass::RenewalTerminate, DETAIL_RENEWAL).into();
            }
        };
        let Some(_turn) = claimed else {
            return failed(FailureClass::StartupPosture, DETAIL_TURN).into();
        };
        let verbs = crate::verbs::FencedVerbs::new(
            &lessee.plane,
            &self.ids.lease,
            self.lease.fencing_token,
            &lessee.halt,
            &self.interrupt,
            &self.decisions,
        );
        let prepared = match lessee
            .agent
            .prepare_task(self.lease, &verbs, &self.meter, &self.interrupt)
            .await
        {
            Ok(prepared) => prepared,
            Err(failure) => {
                return self
                    .fail(
                        &failure,
                        FailureClass::StartupPosture,
                        "task_preparation_failed",
                        "native task intent could not be established",
                    )
                    .into();
            }
        };
        self.with_memory(needs.sandbox, sink, prepared.as_ref())
            .await
    }

    async fn with_memory(
        &self,
        needs_sandbox: bool,
        sink: ActivitySink,
        prepared: Option<&afr_agent::PreparedTask>,
    ) -> Worked {
        let lessee = self.lessee;
        // The window rides the lease from a daemon at this release; only a
        // lease without one costs the memory read, and only that read is timed.
        let read;
        let decoded;
        let memory = if let Some(window) = self.lease.memory.as_ref() {
            afr_telemetry::record::memory_source(afr_telemetry::labels::MemorySource::Inline);
            window
        } else {
            afr_telemetry::record::memory_source(afr_telemetry::labels::MemorySource::Fallback);
            let hydrating = self.timeline.start(Segment::Memory);
            read = match memory::hydrate(&lessee.plane, &self.ids.fleet).await {
                Ok(read) => read,
                Err(failure) => {
                    return self
                        .refuse(&failure, EVENT_HYDRATE_FAILED, DETAIL_MEMORY)
                        .into();
                }
            };
            decoded = match read.decode::<MemoryHydrateResponse<'_>>() {
                Ok(decoded) => decoded,
                Err(failure) => {
                    return self
                        .refuse(&failure, EVENT_HYDRATE_FAILED, DETAIL_MEMORY)
                        .into();
                }
            };
            hydrating.stop();
            &decoded
        };
        let recaller = memory::Recaller::new(&lessee.plane, &self.ids.fleet, self.lease);
        let seed = Seed {
            window: &memory.memory,
            shared: &memory.shared,
            publish: memory.publish,
            recall: Some(&recaller),
        };
        self.execute(needs_sandbox, seed, sink, prepared).await
    }

    async fn execute(
        &self,
        needs_sandbox: bool,
        seed: Seed<'_>,
        sink: ActivitySink,
        prepared: Option<&afr_agent::PreparedTask>,
    ) -> Worked {
        if needs_sandbox && prepared.is_none_or(|prepared| prepared.allow_actions) {
            self.sandboxed(seed, sink, prepared).await
        } else {
            let none = ExecutorSlot::unavailable(Unavailable::NoSandbox);
            let recovered = match self.recover_without_sandbox(prepared).await {
                Ok(recovered) => recovered,
                Err(failure) => {
                    return self
                        .refuse(
                            &failure,
                            "task_restore_failed",
                            "durable task recovery could not be established",
                        )
                        .into();
                }
            };
            self.drive(seed, &none, sink, prepared, recovered.as_ref())
                .await
                .into()
        }
    }
}
