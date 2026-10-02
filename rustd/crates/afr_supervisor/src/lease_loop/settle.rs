//! A finished lease's settle: its memory pushed, its report spooled and posted.

use std::mem;

use afd_core::error_code;
use afd_wire::lease::LeasePayload;
use afd_wire::memory::MemoryDelta;
use bytes::Bytes;
use tokio::time::Instant;

use super::{Ids, Lessee};
use crate::client::retrying;
use crate::memory;
use crate::report::{Ending, report};
use crate::report_spool::{Delivery, Spooled};

const EVENT_CAPTURE_FAILED: &str = "memory_capture_post_failed";
const EVENT_SPOOL_KEPT: &str = "report_spool_kept";
const EVENT_SPOOL_UNAVAILABLE: &str = "report_spool_unavailable";
const EVENT_ENCODE_FAILED: &str = "report_encode_failed";
const EVENT_UNSPOOLED_LOST: &str = "report_failed";

impl Lessee {
    /// Pushes the run's memory, then spools and posts its report.
    pub(super) async fn settle(
        &self,
        ids: &Ids,
        lease: &LeasePayload<'_>,
        ending: &mut Ending,
        started: Instant,
    ) {
        if let Ending::Ran { output, .. } = ending {
            self.capture(ids, lease, mem::take(&mut output.memory))
                .await;
        }
        let report = report(lease, ending, started.elapsed());
        let bytes = match serde_json::to_vec(&report) {
            Ok(bytes) => Bytes::from(bytes),
            Err(failure) => {
                let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
                let lease_id = ids.lease.as_str();
                let reason = failure.to_string();
                let event = EVENT_ENCODE_FAILED;
                tracing::error!(
                    error_code = code,
                    lease_id,
                    reason,
                    event,
                    "the report would not serialize"
                );
                return;
            }
        };
        match self.spool.hold(&ids.lease, bytes.clone()) {
            Ok(spooled) => self.deliver(ids, &spooled).await,
            Err(failure) => self.post_unspooled(ids, bytes, &failure).await,
        }
    }

    /// Pushes the run's memory before the report settles the lease.
    async fn capture(
        &self,
        ids: &Ids,
        lease: &LeasePayload<'_>,
        memory: Vec<MemoryDelta<'static>>,
    ) {
        if let Err(failure) = memory::capture(&self.plane, &ids.fleet, lease, memory).await {
            let code = failure.code().as_str();
            let lease_id = ids.lease.as_str();
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
    /// drain.
    async fn deliver(&self, ids: &Ids, spooled: &Spooled) {
        let failure = match spooled.deliver(&self.plane).await {
            Ok(Delivery::Settled | Delivery::Rejected) => return,
            Ok(Delivery::Kept(failure)) | Err(failure) => failure,
        };
        if !self.halt.stops_on(&failure) {
            let code = failure.code().as_str();
            let lease_id = ids.lease.as_str();
            let event = EVENT_SPOOL_KEPT;
            tracing::warn!(
                error_code = code,
                lease_id,
                event,
                "the report stays spooled; the drain posts it again"
            );
        }
        self.held.notify_one();
    }

    /// The spool would not take the report: post it directly, and take no new
    /// lease, since the next report would have nowhere durable to wait either.
    async fn post_unspooled(&self, ids: &Ids, bytes: Bytes, failure: &crate::Error) {
        let code = failure.code().as_str();
        let lease_id = ids.lease.as_str();
        let event = EVENT_SPOOL_UNAVAILABLE;
        tracing::error!(
            error_code = code,
            lease_id,
            event,
            "the report goes out unspooled"
        );
        self.halt.stop_leasing();
        if let Err(lost) = retrying(|| self.plane.report(bytes.clone())).await
            && !self.halt.stops_on(&lost)
        {
            let code = lost.code().as_str();
            let event = EVENT_UNSPOOLED_LOST;
            tracing::error!(
                error_code = code,
                lease_id,
                event,
                "an unspooled report was not delivered"
            );
        }
    }
}

#[cfg(test)]
#[path = "settle_tests.rs"]
mod tests;
