//! The admit half: commit the row, then hand a fresh one to the receipt half
//! (`admit_receipt`), which appends the entry and records the receipt.

use afd_core::clock::{self, UnixMillis};
use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_observability::metrics::label::fleet::AdmissionOutcome;
use afd_observability::producers::fleet::admission as metrics;
use sqlx::Row as _;

use crate::error::{Error, ErrorKind, Result, query};
use crate::{
    Admission, Admissions, Admitted, BudgetScope, Key, Repeated, Reply, logical_id, logical_parts,
    sql,
};

/// Statement name, for the context a query failure carries.
const CONTEXT_ADMIT: &str = "admit an event";

/// A row's `replay_count` on the day it is admitted.
const NO_REPLAYS: i64 = 0;

/// A [`Reply`] as the four parameters `INSERT_ADMISSION` takes it in.
///
/// At most one pair is set, which is what lets the statement `COALESCE` them
/// without ever mixing a stated half with an inherited one.
pub(crate) struct ReplyBinds<'a> {
    /// `$14`, a stated connector.
    pub(crate) connector: Option<&'a str>,
    /// `$15`, a stated address.
    pub(crate) address: Option<&'a str>,
    /// `$16`, the inherited event's `created_at`.
    pub(crate) inherit_created_at: Option<i64>,
    /// `$17`, the inherited event's `seq`.
    pub(crate) inherit_seq: Option<i64>,
}

impl<'a> From<Reply<'a>> for ReplyBinds<'a> {
    fn from(reply: Reply<'a>) -> Self {
        let (connector, address, inherited) = match reply {
            Reply::None => (None, None, None),
            Reply::To { connector, address } => (Some(connector), Some(address), None),
            // An id this ledger never minted names no row, so it copies none —
            // the same answer as an event recorded without a destination.
            Reply::Inherit { event_id } => (None, None, logical_parts(event_id)),
        };
        let (inherit_created_at, inherit_seq) = inherited.unzip();
        Self {
            connector,
            address,
            inherit_created_at,
            inherit_seq,
        }
    }
}

/// What the ledger answered for one admission.
struct Ledger {
    /// The row the key holds, this statement's or an earlier one's.
    stored: Repeated,
    /// Whether THIS statement created the row.
    inserted: bool,
    /// The receipt already recorded, if any.
    receipt: Option<String>,
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

        let ledger = self
            .commit(&row_id, &admission, key, &digest, now)
            .await
            .inspect_err(|refused| count_refusal(refused, &admission))?;
        let event_id = ledger.stored.id.as_str();
        if ledger.stored.digest != digest && !admission.producer.is_caller_keyed() {
            // The key is the identity and the first payload stands. Logged
            // at warn because a body this daemon renders differently than it
            // did is a deploy that changed a handler, and somebody should
            // know a sender saw both shapes.
            let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
            tracing::warn!(
                error_code = code,
                producer,
                fleet_id = admission.fleet,
                event_id,
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
                event_id,
                event = "admission_replayed",
            );
            return Ok(Admitted {
                replayed: true,
                stored: ledger.stored,
            });
        }

        self.queue_entry(&row_id, ledger.stored, &admission, now)
            .await
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
        let estimate = self.deployment_estimate(now).await?;
        let reply = ReplyBinds::from(admission.reply);
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
            .bind(i64::try_from(estimate).unwrap_or(i64::MAX))
            .bind(reply.connector)
            .bind(reply.address)
            .bind(reply.inherit_created_at)
            .bind(reply.inherit_seq)
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
        if inserted {
            // Only a fresh row joins the set the ceiling counts; the conflict
            // arm updated a row that was already in it, or already receipted.
            self.ceiling.admitted();
        }
        let created_at: i64 = row.try_get(1).map_err(query(CONTEXT_ADMIT))?;
        let seq: i64 = row.try_get(2).map_err(query(CONTEXT_ADMIT))?;
        let receipt: Option<String> = row.try_get(3).map_err(query(CONTEXT_ADMIT))?;
        let digest: String = row.try_get(4).map_err(query(CONTEXT_ADMIT))?;
        let fleet: String = row.try_get(5).map_err(query(CONTEXT_ADMIT))?;
        Ok(Ledger {
            stored: Repeated {
                id: logical_id(created_at, seq),
                digest,
                fleet,
            },
            inserted,
            receipt,
        })
    }
}

/// Counts and logs an admission that recorded nothing.
///
/// Two refusals, kept apart on the counter and in the log: a spent budget is
/// the deployment doing what it was told, and a database that would not answer
/// is an incident.
fn count_refusal(refused: &Error, admission: &Admission<'_>) {
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
    let producer = admission.producer.as_str();
    tracing::warn!(
        error_code = code,
        producer,
        fleet_id = admission.fleet,
        reason,
        event,
    );
}
