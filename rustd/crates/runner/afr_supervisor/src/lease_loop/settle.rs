//! One current-format report stays in its worker until the daemon acknowledges it.

use std::mem;

use af_wire::tool_detail::ToolCallRecord;
use zeroize::Zeroizing;

use super::{LeaseRun, hold::Fate, segments::Stopwatch};
use crate::client::retrying;
use crate::report::{Ending, delivery::until_acknowledged, report};
use crate::{Result, error, memory::CapturedMemory, records, renew};

const EVENT_RECORDS_FAILED: &str = "tool_records_post_failed";

impl LeaseRun<'_> {
    /// Keeping the worker occupied bounds pending reports by admitted leases.
    /// A reconnect retries these same bytes; process loss discards them.
    pub(super) async fn settle(
        &self,
        ending: &mut Ending,
        fate: &Fate,
        settling: &Stopwatch<'_>,
    ) -> Result<()> {
        self.decisions
            .settle(&self.lessee.plane, &self.lessee.halt, &self.ids.lease)
            .await?;
        let (memory, trace) = match ending {
            Ending::Ran { output, .. } => {
                self.post_records(&mem::take(&mut output.records)).await;
                let memory = CapturedMemory::from(mem::take(&mut output.memory));
                let trace = output
                    .trace
                    .take()
                    .map(|trace| {
                        serde_json::to_string(&trace)
                            .map(Zeroizing::new)
                            .map_err(error::encode)
                    })
                    .transpose()?;
                (memory, trace)
            }
            Ending::Failed { .. } => (CapturedMemory::default(), None),
        };
        renew::flush(&self.lessee.plane, &self.ids.lease, &self.meter).await;
        let mut request = report(
            self.lease,
            ending,
            &self.meter,
            self.started.elapsed(),
            trace
                .as_deref()
                .map(|encoded| serde_json::from_str(encoded))
                .transpose()
                .map_err(error::encode)?,
            fate.held_until(),
        );
        request.memory = memory.views();
        request.telemetry.segments = Some(self.timeline.reported(settling.elapsed()));
        let bytes = Zeroizing::new(serde_json::to_vec(&request).map_err(error::encode)?);
        until_acknowledged(&self.lessee.halt, || async {
            self.lessee.plane.report(&bytes).await.map(Some)
        })
        .await
    }

    /// Posts each finished call's full record before the report, so "show
    /// all" has them once the run settles. A post that fails stops the rest;
    /// the report settles regardless, since the answer outweighs the detail.
    async fn post_records(&self, records: &[ToolCallRecord<'static>]) {
        if records.is_empty() {
            return;
        }
        let plane = &self.lessee.plane;
        let posted: crate::Result<()> = async {
            for body in records::bodies(self.lease.fencing_token, records)? {
                retrying(|| plane.tool_calls(&self.ids.lease, body.clone())).await?;
            }
            Ok(())
        }
        .await;
        if let Err(failure) = posted {
            let code = failure.code().as_str();
            let lease_id = self.ids.lease.as_str();
            let event = EVENT_RECORDS_FAILED;
            tracing::warn!(
                error_code = code,
                lease_id,
                event,
                "the run's full tool records were not stored; the report still settles"
            );
        }
    }
}

#[cfg(test)]
#[path = "tests/settlement.rs"]
mod tests;
