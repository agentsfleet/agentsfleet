//! The receipt half of an admission: append the entry a fresh row owns, record
//! the receipt, and mark the fleet leasable.

use afd_core::error_code::{self, Coded as _, Logged};
use afd_core::id::Uuid7;
use afd_dragonfly::{FleetStreams, ReadyIndex};
use afd_observability::metrics::label::fleet::AdmissionOutcome;
use afd_observability::producers::fleet::admission as metrics;
use afd_wire::event::Entry;

use crate::error::{Result, query};
use crate::{Admission, Admissions, Admitted, Repeated, sql};

/// Statement name, for the context a query failure carries.
const CONTEXT_RECEIPT: &str = "record an admission's receipt";

impl Admissions {
    /// Appends the entry this call's row owns and records the receipt.
    ///
    /// A queue that refuses is a deferral: the row is committed, the caller
    /// is answered, and the sweeper appends when the queue is back. A receipt
    /// the row already carries by the time this writes means the sweeper got
    /// there first; the extra physical entry is dropped at lease by the
    /// `fleet.events` conflict arm, and the line below says it happened.
    ///
    /// The entry's `created_at` is the row's, which the database stamped, in
    /// the milliseconds [`Entry::created_at`] is spelled in.
    pub(crate) async fn queue_entry(
        &self,
        row_id: &Uuid7,
        stored: Repeated,
        admission: &Admission<'_>,
    ) -> Result<Admitted> {
        let producer = admission.producer.as_str();
        let event_id = stored.id.as_str();
        let created_at = stored.created_at.as_millisecond().to_string();
        let entry = Entry {
            actor: admission.actor,
            event_type: admission.event_type.as_str(),
            workspace_id: admission.workspace,
            request_json: admission.request_json,
            created_at: &created_at,
        };
        let appended = FleetStreams::new(self.queue.clone())
            .append(admission.fleet, &entry.queued_pairs(event_id))
            .await;
        let receipt = match appended {
            Ok(receipt) => receipt,
            Err(unreachable_queue) => {
                count_deferral(&unreachable_queue, admission, event_id);
                return Ok(fresh(stored));
            }
        };

        let recorded = match self.record_receipt(row_id, receipt.as_str()).await {
            Ok(recorded) => recorded,
            Err(error) => {
                let Logged { error_code, reason } = error.logged();
                tracing::warn!(
                    error_code,
                    reason,
                    producer,
                    event_id,
                    fleet_id = admission.fleet,
                    event = "admission_receipt_deferred",
                    "accepted work awaits receipt recovery"
                );
                metrics::admitted(AdmissionOutcome::Deferred);
                self.mark_ready(admission.fleet).await;
                return Ok(fresh(stored));
            }
        };
        let receipt_field = receipt.as_str();
        if recorded == 0 {
            let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
            tracing::warn!(
                error_code = code,
                producer,
                fleet_id = admission.fleet,
                event_id,
                receipt = receipt_field,
                event = "admission_receipt_superseded",
            );
        }
        metrics::admitted(AdmissionOutcome::Appended);
        tracing::info!(
            producer,
            fleet_id = admission.fleet,
            workspace_id = admission.workspace,
            event_id,
            receipt = receipt_field,
            event = "admission_completed",
        );
        self.mark_ready(admission.fleet).await;
        Ok(fresh(stored))
    }

    async fn record_receipt(&self, row: &Uuid7, receipt: &str) -> Result<u64> {
        let mut connection = self.database.acquire().await?;
        sqlx::query(sql::RECORD_RECEIPT)
            .bind(row.as_str())
            .bind(receipt)
            .execute(&mut *connection)
            .await
            .map(|written| written.rows_affected())
            .map_err(query(CONTEXT_RECEIPT))
    }

    /// Marks the fleet leasable, best-effort.
    ///
    /// The index mints the token, so this mark is a new generation a poll
    /// that peeked the previous one cannot clear. A mark that fails is logged
    /// rather than raised — the entry is already durable, and the streams are
    /// the system of record the poll's backstop asks.
    pub(crate) async fn mark_ready(&self, fleet: &str) {
        if let Err(unmarked) = ReadyIndex::new(self.queue.clone()).mark(fleet).await {
            afd_observability::producers::fleet::ready_write_failed();
            let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
            let Logged {
                error_code: cause_error_code,
                reason,
            } = unmarked.logged();
            tracing::warn!(
                error_code = code,
                cause_error_code,
                fleet_id = fleet,
                reason,
                event = "admission_ready_mark_failed",
            );
        }
    }
}

/// Counts and logs an append the queue would not take.
///
/// A full queue is named as such: the row is just as safe and the sweeper just
/// as owed, but the cure is capacity rather than connectivity, and an operator
/// reads the event name.
fn count_deferral(
    unreachable_queue: &afd_dragonfly::Error,
    admission: &Admission<'_>,
    event_id: &str,
) {
    metrics::admitted(AdmissionOutcome::Deferred);
    let Logged { error_code, reason } = unreachable_queue.logged();
    let producer = admission.producer.as_str();
    let event = if unreachable_queue.is_full() {
        "admission_queue_full"
    } else {
        "admission_append_deferred"
    };
    tracing::warn!(
        error_code,
        producer,
        fleet_id = admission.fleet,
        event_id,
        reason,
        event,
    );
}

/// The answer a row this call inserted gives its producer.
const fn fresh(stored: Repeated) -> Admitted {
    Admitted {
        replayed: false,
        stored,
    }
}
