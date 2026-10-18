//! Queue append and receipt persistence retain one event identity: the row id.

use super::{CONTEXT_REPLAY, Pending};
use crate::{Admissions, Result, error::query, sql};
use afd_core::error_code::{Coded as _, Logged};
use afd_dragonfly::FleetStreams;
use afd_observability::{
    metrics::label::fleet::ReplayOutcome, producers::fleet::admission as metrics,
};
use afd_wire::event::Entry;
use sqlx::{Postgres, Transaction};

impl Admissions {
    /// Appends one pending row and records its receipt inside `transaction`.
    ///
    /// Answers whether the append landed. A queue refusal is logged and
    /// answered `false`; the caller stops the pass there, because a queue
    /// that refused one append will refuse the next.
    pub(crate) async fn append_pending(
        &self,
        pending: &Pending,
        transaction: &mut Transaction<'_, Postgres>,
    ) -> Result<bool> {
        // The stream spells the producer's instant in milliseconds
        // (`Entry::created_at`); the row holds it as a timestamp.
        let created_at = pending.event_created_at.as_millisecond().to_string();
        let entry = Entry {
            actor: &pending.actor,
            // The stored spelling, passed through. It was written by
            // `EventType::as_str` at admit time, and parsing it back only to
            // spell it again would let a row a NEWER daemon admitted be
            // replayed under a type this build happens to know.
            event_type: &pending.event_type,
            workspace_id: &pending.workspace_id,
            request_json: &pending.request_json,
            created_at: &created_at,
        };
        let event_id = pending.id.as_str();
        let fleet_id = pending.fleet_id.as_str();
        let receipt = match FleetStreams::new(self.queue.clone())
            .append(fleet_id, &entry.queued_pairs(event_id))
            .await
        {
            Ok(receipt) => receipt,
            Err(unreachable_queue) => {
                let (outcome, event) = if unreachable_queue.is_full() {
                    (ReplayOutcome::Full, "admission_replay_queue_full")
                } else {
                    (ReplayOutcome::Failed, "admission_replay_failed")
                };
                metrics::replayed(outcome);
                let Logged { error_code, reason } = unreachable_queue.logged();
                tracing::warn!(error_code, fleet_id, event_id, reason, event,);
                return Ok(false);
            }
        };

        // The count comes back so a row that keeps returning is visible as
        // something other than throughput: one re-append is the crash window
        // this sweeper exists for, and a fifth is a row that is being appended
        // and never receipted.
        let replays: i64 = sqlx::query_scalar(sql::RECORD_REPLAY_RECEIPT)
            .bind(pending.id.as_str())
            .bind(receipt.as_str())
            .fetch_one(&mut **transaction)
            .await
            .map_err(query(CONTEXT_REPLAY))?;
        metrics::replayed(ReplayOutcome::Appended);
        let receipt_field = receipt.as_str();
        tracing::info!(
            fleet_id,
            event_id,
            receipt = receipt_field,
            replays,
            event = "admission_replayed_to_queue",
        );
        Ok(true)
    }
}
