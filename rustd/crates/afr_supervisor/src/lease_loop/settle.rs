//! A finished lease's settle: its memory pushed, its report spooled and posted.

use std::mem;

use afd_core::clock::UnixMillis;
use afd_core::error_code;
use afd_wire::memory::MemoryDelta;
use afd_wire::tool_detail::ToolCallRecord;
use bytes::Bytes;
use tokio::time::Instant;

use super::LeaseRun;
use crate::client::retrying;
use crate::memory;
use crate::records;
use crate::report::{Ending, report};
use crate::report_spool::{Delivery, Spooled, settles};

const EVENT_CAPTURE_FAILED: &str = "memory_capture_post_failed";
const EVENT_RECORDS_FAILED: &str = "tool_records_post_failed";
const EVENT_SPOOL_KEPT: &str = "report_spool_kept";
const EVENT_SPOOL_UNAVAILABLE: &str = "report_spool_unavailable";
const EVENT_ENCODE_FAILED: &str = "report_encode_failed";
const EVENT_UNSPOOLED_LOST: &str = "report_failed";

impl LeaseRun<'_> {
    /// Posts the run's full tool records, pushes its memory, then spools and
    /// posts its report, which carries when the sandbox the run left held
    /// lapses. Answers whether the daemon will never record the report, so
    /// the sandbox the run left held carries a run it does not know and serves
    /// no next lease.
    pub(super) async fn settle(
        &self,
        ending: &mut Ending,
        started: Instant,
        held_until: Option<UnixMillis>,
    ) -> bool {
        let mut trace = None;
        if let Ending::Ran { output, .. } = ending {
            self.post_records(&mem::take(&mut output.records)).await;
            self.capture(mem::take(&mut output.memory)).await;
            trace = output
                .trace
                .take()
                .and_then(|trace| serde_json::to_string(&trace).ok());
        }
        let report = report(
            self.lease,
            ending,
            &self.meter,
            started.elapsed(),
            trace.as_deref(),
            held_until,
        );
        let bytes = match serde_json::to_vec(&report) {
            Ok(bytes) => Bytes::from(bytes),
            Err(failure) => {
                let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
                let lease_id = self.ids.lease.as_str();
                let reason = failure.to_string();
                let event = EVENT_ENCODE_FAILED;
                tracing::error!(
                    error_code = code,
                    lease_id,
                    reason,
                    event,
                    "the report would not serialize"
                );
                // Never posted, so never recorded: the hold ends as a rejected
                // report's does. No test drives this arm: a `ReportRequest` is
                // strings, integers, unit enums and parsed raw JSON, none of
                // which `serde_json` refuses to write to a `Vec`.
                return true;
            }
        };
        match self.lessee.spool.hold(&self.ids.lease, bytes.clone()).await {
            Ok(spooled) => self.deliver(&spooled).await,
            Err(failure) => self.post_unspooled(bytes, &failure).await,
        }
    }

    /// Posts each finished call's full record before the report, so "show
    /// all" has them once the run settles. A post that fails stops the rest;
    /// the report settles regardless, since the answer outweighs the detail.
    async fn post_records(&self, records: &[ToolCallRecord<'static>]) {
        if records.is_empty() {
            return;
        }
        let plane = &self.lessee.plane;
        let posted = match records::bodies(self.lease.fencing_token, records) {
            Ok(bodies) => {
                let mut posted = Ok(());
                for body in bodies {
                    posted = retrying(|| plane.tool_calls(&self.ids.lease, body.clone())).await;
                    if posted.is_err() {
                        break;
                    }
                }
                posted
            }
            Err(failure) => Err(failure),
        };
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

    /// Pushes the run's memory before the report settles the lease.
    async fn capture(&self, memory: Vec<MemoryDelta<'static>>) {
        let plane = &self.lessee.plane;
        if let Err(failure) = memory::capture(plane, &self.ids.fleet, self.lease, memory).await {
            let code = failure.code().as_str();
            let lease_id = self.ids.lease.as_str();
            let event = EVENT_CAPTURE_FAILED;
            // Error, not warning: the fleet's next run starts without these
            // items. The report still settles, because withholding it would
            // lose the answer too and let the lease lapse into a re-run.
            tracing::error!(
                error_code = code,
                lease_id,
                event,
                "the run's memory was not written back"
            );
        }
    }

    /// Posts a spooled report once; one the daemon cannot take yet goes to the
    /// drain. Answers whether the daemon will never record it: the lease was
    /// settled without it, or the daemon cannot read it.
    async fn deliver(&self, spooled: &Spooled) -> bool {
        let lessee = self.lessee;
        let failure = match spooled.deliver(&lessee.plane).await {
            Ok(Delivery::Settled) => return false,
            Ok(Delivery::Superseded | Delivery::Rejected) => return true,
            Ok(Delivery::Kept(failure)) | Err(failure) => failure,
        };
        if !lessee.halt.stops_on(&failure) {
            let code = failure.code().as_str();
            let lease_id = self.ids.lease.as_str();
            let event = EVENT_SPOOL_KEPT;
            tracing::warn!(
                error_code = code,
                lease_id,
                event,
                "the report stays spooled; the drain posts it again"
            );
        }
        lessee.held.notify_one();
        false
    }

    /// The spool would not take the report: post it directly, and take no new
    /// lease, since the next report would have nowhere durable to wait either.
    /// Answers whether the daemon will never record it: with no spool to wait
    /// in, a report it did not take is lost.
    async fn post_unspooled(&self, bytes: Bytes, failure: &crate::Error) -> bool {
        let lessee = self.lessee;
        let code = failure.code().as_str();
        let lease_id = self.ids.lease.as_str();
        let event = EVENT_SPOOL_UNAVAILABLE;
        tracing::error!(
            error_code = code,
            lease_id,
            event,
            "the report goes out unspooled"
        );
        lessee.halt.stop_leasing();
        let Err(lost) = retrying(|| lessee.plane.report(bytes.clone())).await else {
            return false;
        };
        if !settles(&lost) && !lessee.halt.stops_on(&lost) {
            let code = lost.code().as_str();
            let event = EVENT_UNSPOOLED_LOST;
            tracing::error!(
                error_code = code,
                lease_id,
                event,
                "an unspooled report was not delivered"
            );
        }
        true
    }
}

#[cfg(test)]
#[path = "settle_tests.rs"]
mod tests;
