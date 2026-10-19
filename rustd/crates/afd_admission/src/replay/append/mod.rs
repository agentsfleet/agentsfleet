//! Queue append and receipt persistence retain one event identity: the row id.
//!
//! The append and the receipt never share a transaction: a claimed row is
//! appended with no Postgres transaction open, and the pass's receipts are
//! recorded afterwards in one statement.

use super::pass::Drained;
use super::{CONTEXT_REPLAY, Pending};
use crate::{Admissions, Result, error::query, sql};
use afd_core::error_code::{Coded as _, Logged};
use afd_dragonfly::{EventId, FleetStreams};
use afd_observability::{
    metrics::label::fleet::ReplayOutcome, producers::fleet::admission as metrics,
};
use afd_wire::event::Entry;
use sqlx::PgConnection;

/// One row [`sql::RECORD_REPLAY_RECEIPTS`] recorded, by column name.
#[derive(Debug, sqlx::FromRow)]
struct Recorded {
    id: String,
    replay_count: i64,
}

impl Admissions {
    /// Appends one claimed row to its fleet's stream and answers the receipt.
    ///
    /// A queue refusal is counted, logged and answered `None`; the pass stops
    /// there, because a queue that refused one append will refuse the next.
    pub(super) async fn append_pending(&self, pending: &Pending) -> Option<EventId> {
        // The stream spells the producer's instant in RFC 3339
        // (`Entry::created_at`); the row holds it as a timestamp.
        let created_at = pending.event_created_at.to_string();
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
        match FleetStreams::new(self.queue.clone())
            .append(fleet_id, &entry.queued_pairs(event_id))
            .await
        {
            Ok(receipt) => {
                metrics::replayed(ReplayOutcome::Appended);
                Some(receipt)
            }
            Err(unreachable_queue) => {
                let (outcome, event) = if unreachable_queue.is_full() {
                    (ReplayOutcome::Full, "admission_replay_queue_full")
                } else {
                    (ReplayOutcome::Failed, "admission_replay_failed")
                };
                metrics::replayed(outcome);
                let Logged { error_code, reason } = unreachable_queue.logged();
                tracing::warn!(error_code, fleet_id, event_id, reason, event,);
                None
            }
        }
    }

    /// Records the receipts a pass's appends answered with, then releases
    /// every claim the pass did not turn into a receipt.
    ///
    /// A row missing from the recorded set had its receipt written by its
    /// inserter while the pass appended; its entry here is the extra one,
    /// which delivery drops by the event id, and the line below says so.
    ///
    /// # Errors
    /// Reports a datastore that would not answer. The appended entries are
    /// already on the stream. Every claim is released on a best-effort basis
    /// first, so the next pass or confirmation retries at once; if that fails
    /// too, the claims lapse after the claim span. Either way the rows are
    /// appended again, which delivery drops by the event id.
    pub(super) async fn record_replays(&self, drained: &Drained<'_>) -> Result<()> {
        let mut connection = self.database.acquire().await?;
        let recorded = match record(&mut connection, drained).await {
            Ok(recorded) => recorded,
            Err(unrecorded) => {
                let claimed: Vec<&str> = drained
                    .appended
                    .iter()
                    .map(|(pending, _)| pending.id.as_str())
                    .chain(drained.unreached.iter().map(|pending| pending.id.as_str()))
                    .collect();
                // The record's failure is the one to report; a release that
                // fails as well only means the claims lapse instead.
                let _released = release(&mut connection, &claimed).await;
                return Err(unrecorded);
            }
        };

        let mut released: Vec<&str> = drained
            .unreached
            .iter()
            .map(|pending| pending.id.as_str())
            .collect();
        for (pending, receipt) in &drained.appended {
            let fleet_id = pending.fleet_id.as_str();
            let event_id = pending.id.as_str();
            let receipt = receipt.as_str();
            // The count comes back so a row that keeps returning is visible
            // as something other than throughput: one re-append is the crash
            // window this sweeper exists for, and a fifth is a row that is
            // being appended and never receipted.
            if let Some(row) = recorded.iter().find(|row| row.id == pending.id) {
                tracing::info!(
                    fleet_id,
                    event_id,
                    receipt,
                    replays = row.replay_count,
                    event = "admission_replayed_to_queue",
                );
            } else {
                released.push(event_id);
                tracing::info!(
                    fleet_id,
                    event_id,
                    receipt,
                    event = "admission_replay_superseded",
                );
            }
        }
        release(&mut connection, &released).await
    }
}

/// Records `drained`'s receipts in one statement; the rows it answers are the
/// ones this call receipted.
async fn record(connection: &mut PgConnection, drained: &Drained<'_>) -> Result<Vec<Recorded>> {
    if drained.appended.is_empty() {
        return Ok(Vec::new());
    }
    let (ids, receipts): (Vec<&str>, Vec<&str>) = drained
        .appended
        .iter()
        .map(|(pending, receipt)| (pending.id.as_str(), receipt.as_str()))
        .unzip();
    sqlx::query_as(sql::RECORD_REPLAY_RECEIPTS)
        .bind(&ids)
        .bind(&receipts)
        .fetch_all(&mut *connection)
        .await
        .map_err(query(CONTEXT_REPLAY))
}

/// Clears the claims on `ids`; none is no statement.
async fn release(connection: &mut PgConnection, ids: &[&str]) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    sqlx::query(sql::RELEASE_REPLAY_CLAIMS)
        .bind(ids)
        .execute(&mut *connection)
        .await
        .map(drop)
        .map_err(query(CONTEXT_REPLAY))
}

#[cfg(test)]
mod tests;
