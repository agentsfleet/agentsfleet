//! One current-format report stays in its worker until the daemon acknowledges it.

use std::mem;

use af_wire::tool_detail::ToolCallRecord;
use zeroize::Zeroizing;

use super::{Run, hold::Fate, segments::Stopwatch};
use crate::report::{Ending, delivery::until_acknowledged, report};
use crate::{Result, error, memory::CapturedMemory, records, renew};

const EVENT_RECORDS_FAILED: &str = "tool_records_post_failed";

impl Run<'_> {
    /// Keeping the worker occupied bounds pending reports by admitted runs.
    /// A reconnect retries these same bytes; process loss discards them.
    pub(super) async fn settle(
        &self,
        ending: &mut Ending,
        fate: &Fate,
        settling: &Stopwatch<'_>,
    ) -> Result<()> {
        self.decisions
            .settle(&self.lessee.plane, &self.lessee.halt, &self.ids.run)
            .await?;
        let (memory, trace) = match ending.output_mut() {
            Some(output) => {
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
            None => (CapturedMemory::default(), None),
        };
        renew::flush(&self.lessee.plane, &self.ids.run, &self.meter).await;
        let mut request = report(
            self.payload,
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
        let response =
            until_acknowledged(&self.lessee.halt, self.lessee.plane.runtime, async || {
                self.lessee.plane.report(&bytes).await.map(Some)
            })
            .await?;
        let accepted = response.hold.as_ref().is_some_and(|hold| {
            hold.run_id == self.ids.run.as_str()
                && hold.fencing_token == self.payload.fencing_token
                && hold.expires_at > self.lessee.clock.now()
                && fate
                    .held_until()
                    .is_some_and(|requested| hold.expires_at <= requested)
        });
        if let Some(hold) = response.hold.filter(|_| accepted) {
            self.lessee
                .holds
                .accept(self.ids.run.clone(), hold.expires_at);
        } else {
            self.lessee.holds.supersede(self.ids.run.clone());
        }
        Ok(())
    }

    /// Posts each finished call's full record before the report, so "show
    /// all" has them once the run settles. A post that fails stops the rest;
    /// the report settles regardless, since the answer outweighs the detail.
    async fn post_records(&self, records: &[ToolCallRecord<'static>]) {
        if records.is_empty() {
            return;
        }
        let plane = &self.lessee.plane;
        let posted: Result<()> = async {
            for body in records::bodies(self.payload.fencing_token, records)? {
                plane
                    .retrying(|| plane.tool_calls(&self.ids.run, body.clone()))
                    .await?;
            }
            Ok(())
        }
        .await;
        if let Err(failure) = posted {
            let code = failure.code().as_str();
            let run_id = self.ids.run.as_str();
            let event = EVENT_RECORDS_FAILED;
            {
                let (error_code, message) = (
                    &code,
                    format_args!(
                        "the run's full tool records were not stored; the report still settles"
                    ),
                );
                tracing::warn!(message, error_code, run_id, event);
            };
        }
    }
}

#[cfg(test)]
#[path = "tests/settlement.rs"]
mod tests;
