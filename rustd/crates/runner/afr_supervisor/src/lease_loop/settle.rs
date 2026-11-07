//! A finished lease's settle: its memory pushed, its report spooled and posted.

use std::mem;

use af_core::error_code;
use af_wire::memory::MemoryDelta;
use af_wire::report::RunSegments;
use af_wire::tool_detail::ToolCallRecord;
use bytes::Bytes;

use super::LeaseRun;
use super::hold::Fate;
use super::segments::Stopwatch;
use crate::client::retrying;
use crate::memory::{self, CapturedMemory};
use crate::records;
use crate::renew;
use crate::report::{Ending, report};
use crate::report_spool::{Delivery, Spooled, settles};

const EVENT_CAPTURE_FAILED: &str = "memory_capture_post_failed";
const EVENT_RECORDS_FAILED: &str = "tool_records_post_failed";
const EVENT_SPOOL_KEPT: &str = "report_spool_kept";
const EVENT_SPOOL_UNAVAILABLE: &str = "report_spool_unavailable";
const EVENT_UNSPOOLED_LOST: &str = "report_failed";
const EVENT_SHAPE_FELL_BACK: &str = "report_shape_fell_back";
const EVENT_REJECTED_KEPT: &str = "report_spool_rejected_kept";

impl LeaseRun<'_> {
    /// Posts the run's full tool records, then settles: on a lease that
    /// carried its memory window, one report post carrying the run's memory
    /// and segments; on any other, the memory push and then the report. Renews
    /// until its usage lines fit one body, spools the report and posts it; the
    /// report carries when the sandbox the run left held lapses. Answers
    /// whether the daemon will never record the report, so the sandbox the run
    /// left held carries a run it does not know and serves no next lease.
    pub(super) async fn settle(
        &self,
        ending: &mut Ending,
        fate: &Fate,
        settling: &Stopwatch<'_>,
    ) -> bool {
        let held_until = fate.held_until();
        let mut trace = None;
        // Only a run that ran has memory to hand on; a lease that failed
        // before it pushes nothing, as before.
        let mut memory = None;
        if let Ending::Ran { output, .. } = ending {
            self.post_records(&mem::take(&mut output.records)).await;
            memory = Some(CapturedMemory::from(mem::take(&mut output.memory)));
            trace = output
                .trace
                .take()
                .and_then(|trace| serde_json::to_string(&trace).ok());
        }
        // Only a daemon at the release that reads the report's memory and
        // segments puts a window on the lease, so the window is the proof the
        // one-post body will be read; any other lease settles as before.
        let one_post = self.lease.memory.is_some();
        if let Some(memory) = memory.as_ref().filter(|_| !one_post) {
            self.capture(memory).await;
        }
        renew::flush(&self.lessee.plane, &self.ids.lease, &self.meter).await;
        let segments = one_post.then(|| self.timeline.reported(settling.elapsed()));
        let carried = segments.map(|segments| {
            let views = memory.as_ref().map(CapturedMemory::views);
            (views.unwrap_or_default(), segments)
        });
        let bytes = self.report_bytes(ending, trace.as_deref(), held_until, carried);
        match self.post(bytes).await {
            Posted::ShapeRefused if one_post => {
                self.fall_back(ending, trace.as_deref(), held_until, memory.as_ref())
                    .await
            }
            Posted::Recorded => false,
            Posted::ShapeRefused | Posted::Lost => true,
        }
    }

    /// The report body: `carried` adds the run's memory and segments for a
    /// daemon proved to read them.
    fn report_bytes(
        &self,
        ending: &Ending,
        trace: Option<&str>,
        held_until: Option<jiff::Timestamp>,
        carried: Option<(Vec<MemoryDelta<'_>>, RunSegments)>,
    ) -> Bytes {
        let mut report = report(
            self.lease,
            ending,
            &self.meter,
            self.started.elapsed(),
            trace,
            held_until,
        );
        if let Some((memory, segments)) = carried {
            report.memory = memory;
            report.telemetry.segments = Some(segments);
        }
        // A `ReportRequest` is strings, integers, unit enums and parsed raw
        // JSON, none of which `serde_json` refuses to write to a `Vec`, so no
        // run reaches an encode failure. Were one ever to, the empty body it
        // leaves is a report the daemon cannot read: the spool sets it aside
        // for an operator and the hold ends as a rejected report's does.
        Bytes::from(serde_json::to_vec(&report).unwrap_or_default())
    }

    /// Spools `bytes` and posts them once; with no spool, posts them directly.
    async fn post(&self, bytes: Bytes) -> Posted {
        match self.lessee.spool.hold(&self.ids.lease, bytes.clone()).await {
            Ok(spooled) => self.deliver(&spooled).await,
            Err(failure) => self.post_unspooled(bytes, &failure).await,
        }
    }

    /// A one-post report an older daemon refused for its shape, mid-deploy:
    /// the memory goes through the push, the report again without the fields
    /// that daemon does not read, once. The refused copy is removed once its
    /// replacement is spooled, since it holds the run's memory in clear.
    async fn fall_back(
        &self,
        ending: &Ending,
        trace: Option<&str>,
        held_until: Option<jiff::Timestamp>,
        memory: Option<&CapturedMemory>,
    ) -> bool {
        let lease_id = self.ids.lease.as_str();
        let cause_error_code = error_code::INVALID_REQUEST.as_str();
        let event = EVENT_SHAPE_FELL_BACK;
        tracing::warn!(
            cause_error_code,
            lease_id,
            event,
            "a daemon refused the one-post report; the memory is pushed and the report posted without it"
        );
        if let Some(memory) = memory {
            self.capture(memory).await;
        }
        let bytes = self.report_bytes(ending, trace, held_until, None);
        let posted = self.post(bytes).await;
        if let Err(failure) = self.lessee.spool.forget_rejected(&self.ids.lease).await {
            let code = failure.code().as_str();
            let event = EVENT_REJECTED_KEPT;
            tracing::warn!(
                error_code = code,
                lease_id,
                event,
                "the refused one-post copy stays in the spool for the boot sweep"
            );
        }
        posted != Posted::Recorded
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

    /// Pushes the run's memory before the report settles the lease.
    async fn capture(&self, memory: &CapturedMemory) {
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
    async fn deliver(&self, spooled: &Spooled) -> Posted {
        let lessee = self.lessee;
        let failure = match spooled.deliver(&lessee.plane).await {
            Ok(Delivery::Settled) => return Posted::Recorded,
            Ok(Delivery::Superseded) => return Posted::Lost,
            Ok(Delivery::Rejected(refusal)) => return Posted::refused(&refusal),
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
        Posted::Recorded
    }

    /// The spool would not take the report: post it directly, and take no new
    /// lease, since the next report would have nowhere durable to wait either.
    /// Answers whether the daemon will never record it: with no spool to wait
    /// in, a report it did not take is lost.
    async fn post_unspooled(&self, bytes: Bytes, failure: &crate::Error) -> Posted {
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
            return Posted::Recorded;
        };
        if Posted::refused(&lost) == Posted::ShapeRefused {
            return Posted::ShapeRefused;
        }
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
        Posted::Lost
    }
}

/// What became of one report post, as the settle decides on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Posted {
    /// The daemon recorded it, or will once the drain posts it again.
    Recorded,
    /// The daemon will never record it.
    Lost,
    /// The daemon refused the body's shape (`UZ-REQ-001`): an older daemon
    /// that does not read the one-post fields.
    ShapeRefused,
}

impl Posted {
    /// A refusal, as the settle reads it.
    fn refused(refusal: &crate::Error) -> Self {
        if refusal.code() == error_code::INVALID_REQUEST {
            Self::ShapeRefused
        } else {
            Self::Lost
        }
    }
}

#[cfg(test)]
#[path = "tests/settlement.rs"]
mod tests;
