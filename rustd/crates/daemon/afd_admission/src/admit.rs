//! Commit acceptance, then publish a bounded best-effort notification.

use af_core::clock;
use af_core::error_code::{self, Coded as _, Logged};
use af_core::id::Uuid7;
use af_observability::metrics::label::fleet::AdmissionOutcome;
use af_observability::producers::fleet::admission as metrics;
use afd_fleet_lifecycle::FleetStatus;
use jiff::Timestamp;

use crate::error::{Error, Result};
use crate::{Admission, Admissions, Admitted, Key, Repeated, Reply};

/// A [`Reply`] as the three parameters `ADMIT_ONE` takes it in.
///
/// At most one is set, which is what lets the statement `COALESCE` them
/// without ever mixing a stated half with an inherited one.
pub(crate) struct ReplyBinds<'a> {
    /// `$12`, a stated connector.
    pub(crate) connector: Option<&'a str>,
    /// `$13`, a stated address.
    pub(crate) address: Option<&'a str>,
    /// `$14`, the row the inherited event was admitted as.
    pub(crate) inherited: Option<Uuid7>,
}

impl<'a> From<Reply<'a>> for ReplyBinds<'a> {
    fn from(reply: Reply<'a>) -> Self {
        let (connector, address, inherited) = match reply {
            Reply::None => (None, None, None),
            Reply::To { connector, address } => (Some(connector), Some(address), None),
            Reply::Inherit { event_id } => (None, None, Some(event_id.clone())),
        };
        Self {
            connector,
            address,
            inherited,
        }
    }
}

/// What the ledger answered for one admission.
#[derive(Debug)]
pub(crate) struct Ledger {
    /// The row the key holds, this statement's or an earlier one's.
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
    /// Reports a database that would not commit the row, and a spent
    /// deployment ceiling — both the retryable refusal, with no acceptance
    /// recorded — plus a fleet outside the caller's workspace and a setup
    /// needing review. A lost wake notification leaves the committed admission
    /// available to every poll.
    pub async fn admit(&self, admission: Admission<'_>) -> Result<Admitted> {
        self.admit_under(admission, None).await
    }

    /// [`Admissions::admit`] for a fleet that must be runnable: a new key on a
    /// fleet in any other status is refused `NotRunnable`, while a repeat is
    /// still answered.
    ///
    /// # Errors
    /// As [`Admissions::admit`], and `NotRunnable` with the fleet's status.
    pub async fn admit_runnable(&self, admission: Admission<'_>) -> Result<Admitted> {
        self.admit_under(admission, Some(FleetStatus::Active)).await
    }

    async fn admit_under(
        &self,
        admission: Admission<'_>,
        runnable: Option<FleetStatus>,
    ) -> Result<Admitted> {
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
            .commit(&row_id, &admission, key, &digest, now, runnable)
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
                fleet_id = %admission.fleet,
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
            fleet_id = %admission.fleet,
            event_id,
            event = "admission_committed",
            replayed
        );
        Ok(Admitted {
            replayed,
            stored: ledger.stored,
        })
    }

    /// Commit acceptance under the deployment ceiling, in one autocommit statement.
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
        runnable: Option<FleetStatus>,
    ) -> Result<Ledger> {
        let estimate = self.deployment_estimate(now).await;
        let mut connection = self.database.acquire().await?;
        let ledger = self
            .insert_on(
                &mut connection,
                row_id,
                admission,
                key,
                digest,
                crate::stage::insert::Gates { estimate, runnable },
            )
            .await?;
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
    // A missing or stopped fleet is the caller's answer, not an admission
    // failure: the ledger's own statement now refuses both, where the route
    // once did before the ledger saw them.
    if refused.is_fleet_absent() || refused.not_runnable().is_some() {
        return;
    }
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
        fleet_id = %admission.fleet,
        reason,
        event,
    );
}
