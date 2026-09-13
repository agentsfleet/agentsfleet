//! The admit half: commit the row, append the entry, record the receipt.

use afd_core::clock::{self, UnixMillis};
use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_datastore::{FleetStreams, ReadyIndex};
use afd_observability::metrics::label::fleet::AdmissionOutcome;
use afd_observability::producers::fleet::admission as metrics;
use afd_wire::event::Entry;
use sqlx::Row as _;

use crate::error::{Error, ErrorKind, Result, query};
use crate::{Admission, Admissions, Admitted, BudgetScope, Key, logical_id, sql};

/// Statement name, for the context a query failure carries.
const CONTEXT_ADMIT: &str = "admit an event";

/// Statement name, for the context a query failure carries.
const CONTEXT_RECEIPT: &str = "record an admission's receipt";

/// A row's `replay_count` on the day it is admitted.
const NO_REPLAYS: i64 = 0;

/// What the ledger answered for one admission.
struct Ledger {
    /// The logical event id.
    id: String,
    /// Whether THIS statement created the row.
    inserted: bool,
    /// The receipt already recorded, if any.
    receipt: Option<String>,
    /// The digest the row was admitted with.
    digest: String,
}

impl Admissions {
    /// Admits one unit of work, at most once per producer key.
    ///
    /// Answers the logical event id and whether an earlier call already
    /// admitted the key. A caller answers success either way: a producer
    /// retrying work this daemon already holds has nothing to fix.
    ///
    /// # Errors
    /// Reports a database that would not commit the row, and a budget that
    /// is spent — both the retryable refusal, with no acceptance recorded. A
    /// queue that would not take the append is NOT an error: the row is
    /// committed, and the replay sweeper appends it.
    pub async fn admit(&self, admission: Admission<'_>) -> Result<Admitted> {
        let now = clock::now();
        let row_id = Uuid7::encode(now, self.entropy.uuid_randomness()?)?;
        // An unrepeatable producer is keyed on the row it is about to write,
        // so its unique index still holds and this call is still the only
        // one that can own it. Minted HERE and not by the caller: the
        // entropy is the ledger's, so a deployment cannot end up with two
        // sources that could disagree.
        let key = match admission.key {
            Key::Repeated(key) => key,
            Key::Unrepeatable => row_id.as_str(),
        };
        let digest = admission.payload_digest();
        let producer = admission.producer.as_str();

        let ledger = match self.commit(&row_id, &admission, key, &digest, now).await {
            Ok(ledger) => ledger,
            Err(refused) => {
                // Two refusals, kept apart on the counter and in the log: a
                // spent budget is the deployment doing what it was told,
                // and a database that would not answer is an incident.
                let (outcome, event) = if refused.is_over_capacity() {
                    (
                        AdmissionOutcome::OverBudget,
                        "admission_refused_over_capacity",
                    )
                } else {
                    (AdmissionOutcome::Refused, "admission_failed")
                };
                metrics::admitted(outcome);
                let code = refused.code().as_str();
                let reason = refused.to_string();
                tracing::warn!(
                    error_code = code,
                    producer,
                    fleet_id = admission.fleet,
                    reason,
                    event,
                );
                return Err(refused);
            }
        };
        if ledger.digest != digest {
            // The key is the identity and the first payload stands. Logged
            // at warn because a body this daemon renders differently than it
            // did is a deploy that changed a handler, and somebody should
            // know a sender saw both shapes.
            let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
            tracing::warn!(
                error_code = code,
                producer,
                fleet_id = admission.fleet,
                event_id = ledger.id,
                event = "admission_payload_drifted",
            );
        }
        if ledger.receipt.is_some() || !ledger.inserted {
            // Seen before, or another daemon is inserting this very key and
            // owns its append. Either way the first call's id stands.
            metrics::admitted(AdmissionOutcome::Replayed);
            tracing::debug!(
                producer,
                fleet_id = admission.fleet,
                event_id = ledger.id,
                event = "admission_replayed",
            );
            return Ok(Admitted {
                id: ledger.id,
                replayed: true,
            });
        }

        self.queue_entry(&row_id, ledger.id, &admission, now).await
    }

    /// The fleet budget, then the row: the queue is asked first because a
    /// refusal must leave nothing behind, and the row's own statement carries
    /// the deployment budget.
    async fn commit(
        &self,
        row_id: &Uuid7,
        admission: &Admission<'_>,
        key: &str,
        digest: &str,
        now: UnixMillis,
    ) -> Result<Ledger> {
        self.refuse_over_fleet_budget(admission.fleet).await?;
        self.record(row_id, admission, key, digest, now).await
    }

    /// Commits the row, or finds the one an earlier call committed, or
    /// answers the deployment budget's refusal when the statement inserted
    /// nothing.
    async fn record(
        &self,
        row_id: &Uuid7,
        admission: &Admission<'_>,
        key: &str,
        digest: &str,
        now: UnixMillis,
    ) -> Result<Ledger> {
        let mut connection = self.database.acquire().await?;
        let row = sqlx::query(sql::INSERT_ADMISSION)
            .bind(row_id.as_str())
            .bind(admission.fleet)
            .bind(admission.workspace)
            .bind(admission.producer.as_str())
            .bind(key)
            .bind(digest)
            .bind(admission.actor)
            .bind(admission.event_type.as_str())
            .bind(admission.request_json)
            .bind(now.as_millis())
            .bind(NO_REPLAYS)
            .bind(i64::try_from(self.budgets.replay_backlog).unwrap_or(i64::MAX))
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_ADMIT))?
            .ok_or_else(|| {
                Error::from(ErrorKind::OverBudget {
                    scope: BudgetScope::Deployment,
                    limit: self.budgets.replay_backlog,
                })
            })?;
        let inserted: bool = row.try_get(0).map_err(query(CONTEXT_ADMIT))?;
        let created_at: i64 = row.try_get(1).map_err(query(CONTEXT_ADMIT))?;
        let seq: i64 = row.try_get(2).map_err(query(CONTEXT_ADMIT))?;
        let receipt: Option<String> = row.try_get(3).map_err(query(CONTEXT_ADMIT))?;
        let stored: String = row.try_get(4).map_err(query(CONTEXT_ADMIT))?;
        Ok(Ledger {
            id: logical_id(created_at, seq),
            inserted,
            receipt,
            digest: stored,
        })
    }

    /// Appends the entry this call's row owns and records the receipt.
    ///
    /// A queue that refuses is a deferral: the row is committed, the caller
    /// is answered, and the sweeper appends when the queue is back. A receipt
    /// the row already carries by the time this writes means the sweeper got
    /// there first; the extra physical entry is dropped at lease by the
    /// `core.fleet_events` conflict arm, and the line below says it happened.
    async fn queue_entry(
        &self,
        row_id: &Uuid7,
        id: String,
        admission: &Admission<'_>,
        now: UnixMillis,
    ) -> Result<Admitted> {
        let producer = admission.producer.as_str();
        let event_id = id.as_str();
        let created_at = now.as_millis().to_string();
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
                metrics::admitted(AdmissionOutcome::Deferred);
                let code = unreachable_queue.code().as_str();
                let reason = unreachable_queue.to_string();
                // A full queue is named as such: the row is just as safe and
                // the sweeper just as owed, but the cure is capacity rather
                // than connectivity, and an operator reads the event name.
                let event = if unreachable_queue.is_full() {
                    "admission_queue_full"
                } else {
                    "admission_append_deferred"
                };
                tracing::warn!(
                    error_code = code,
                    producer,
                    fleet_id = admission.fleet,
                    event_id,
                    reason,
                    event,
                );
                return Ok(Admitted {
                    id,
                    replayed: false,
                });
            }
        };

        let mut connection = self.database.acquire().await?;
        let recorded = sqlx::query(sql::RECORD_RECEIPT)
            .bind(row_id.as_str())
            .bind(receipt.as_str())
            .bind(now.as_millis())
            .execute(&mut *connection)
            .await
            .map_err(query(CONTEXT_RECEIPT))?;
        let receipt_field = receipt.as_str();
        if recorded.rows_affected() == 0 {
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
        Ok(Admitted {
            id,
            replayed: false,
        })
    }

    /// Marks the fleet leasable, best-effort.
    ///
    /// The token is the fleet id, as every producer spells it: the clear
    /// compares it, so a mark written under another value is one nothing can
    /// remove. A mark that fails is logged rather than raised — the entry is
    /// already durable, and the streams are the system of record the poll's
    /// backstop asks.
    pub(crate) async fn mark_ready(&self, fleet: &str) {
        if let Err(unmarked) = ReadyIndex::new(self.queue.clone()).mark(fleet, fleet).await {
            afd_observability::producers::fleet::ready_write_failed();
            let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
            let reason = unmarked.to_string();
            tracing::warn!(
                error_code = code,
                fleet_id = fleet,
                reason,
                event = "admission_ready_mark_failed",
            );
        }
    }
}
