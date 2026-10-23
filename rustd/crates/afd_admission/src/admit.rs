//! Commit acceptance, then publish a bounded best-effort notification.

use afd_core::clock;
use afd_core::error_code::{self, Coded as _, Logged};
use afd_core::id::Uuid7;
use afd_observability::metrics::label::fleet::AdmissionOutcome;
use afd_observability::producers::fleet::admission as metrics;
use jiff::Timestamp;
use sqlx::Acquire as _;

use crate::error::{Error, Result};
use crate::{Admission, Admissions, Admitted, Key, Repeated, Reply};

/// A [`Reply`] as the three parameters `INSERT_ADMISSION` takes it in.
///
/// At most one is set, which is what lets the statement `COALESCE` them
/// without ever mixing a stated half with an inherited one.
pub(crate) struct ReplyBinds<'a> {
    /// `$13`, a stated connector.
    pub(crate) connector: Option<&'a str>,
    /// `$14`, a stated address.
    pub(crate) address: Option<&'a str>,
    /// `$15`, the row the inherited event was admitted as.
    pub(crate) inherited: Option<Uuid7>,
}

impl<'a> From<Reply<'a>> for ReplyBinds<'a> {
    fn from(reply: Reply<'a>) -> Self {
        let (connector, address, inherited) = match reply {
            Reply::None => (None, None, None),
            Reply::To { connector, address } => (Some(connector), Some(address), None),
            // An id that is not a row id names no row, so it copies none —
            // the same answer as an event recorded without a destination.
            // Parsed here so text that is not a UUID never reaches the
            // statement's `::uuid` cast as a refusal.
            Reply::Inherit { event_id } => (None, None, Uuid7::parse(event_id).ok()),
        };
        Self {
            connector,
            address,
            inherited,
        }
    }
}

/// What the ledger answered for one admission, as `INSERT_ADMISSION`'s
/// `RETURNING` names it.
#[derive(sqlx::FromRow)]
pub(crate) struct Ledger {
    /// The row the key holds, this statement's or an earlier one's.
    #[sqlx(flatten)]
    pub(crate) stored: Repeated,
    /// Whether THIS statement created the row.
    pub(crate) inserted: bool,
}

impl Admissions {
    /// Admits one unit of work, at most once per producer key.
    ///
    /// Answers the event id and whether an earlier call already
    /// admitted the key. A caller answers success either way: a producer
    /// retrying work this daemon already holds has nothing to fix.
    ///
    /// # Errors
    /// Reports a database that would not commit the row, and a budget that
    /// is spent — both the retryable refusal, with no acceptance recorded. A lost
    /// wake notification leaves the committed admission available to every poll.
    pub async fn admit(&self, admission: Admission<'_>) -> Result<Admitted> {
        let now = clock::now();
        let row_id = self.entropy.uuid7(now)?;
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
        self.wake(&ledger.stored.fleet).await;
        metrics::admitted(if ledger.inserted {
            AdmissionOutcome::Accepted
        } else {
            AdmissionOutcome::Replayed
        });
        let replayed = !ledger.inserted;
        tracing::debug!(
            producer,
            fleet_id = admission.fleet,
            event_id,
            event = "admission_committed",
            replayed
        );
        Ok(Admitted {
            replayed,
            stored: ledger.stored,
        })
    }

    /// Commit acceptance under the fleet and deployment backlog budgets.
    ///
    /// `now` is the instant the row id was minted at, and the one the
    /// deployment ceiling's sampling cadence is read against; the row's own
    /// stamps are the database's.
    async fn commit(
        &self,
        row_id: &Uuid7,
        admission: &Admission<'_>,
        key: &str,
        digest: &str,
        now: Timestamp,
    ) -> Result<Ledger> {
        let estimate = self.deployment_estimate(now).await;
        let mut connection = self.database.acquire().await?;
        let mut transaction = connection
            .begin()
            .await
            .map_err(crate::error::query("begin admission"))?;
        let ledger = self
            .insert_on(&mut transaction, row_id, admission, key, digest, estimate)
            .await?;
        transaction
            .commit()
            .await
            .map_err(crate::error::query("commit admission"))?;
        if ledger.inserted {
            self.ceiling.admitted();
        }
        Ok(ledger)
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
    let Logged { error_code, reason } = refused.logged();
    let producer = admission.producer.as_str();
    tracing::warn!(
        error_code,
        producer,
        fleet_id = admission.fleet,
        reason,
        event,
    );
}
