//! Commit acceptance, then publish a bounded best-effort notification.

use af_core::clock;
use af_core::error_code::{self, Coded as _, Logged};
use af_core::id::Uuid7;
use af_observability::metrics::label::fleet::IncomingTurnOutcome;
use af_observability::producers::fleet::turn as metrics;
use afd_db::Precondition;
use afd_fleet_lifecycle::FleetStatus;
use jiff::Timestamp;

use crate::error::{Error, Result, query};
use crate::stage::insert::Gates;
use crate::{Admitted, IncomingTurn, Key, Repeated, Reply, Turns};

/// What a failed guarded intake records against.
const CONTEXT_GUARDED: &str = "admit one message behind a guard";

/// A guarded turn whose guard did not hold: nothing was written.
///
/// A value inside `Ok`, not an error kind, because it is the guard's answer
/// rather than a ledger failure: the caller refuses its own request for it,
/// and no intake refusal is counted or logged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unheld;

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
            Reply::Inherit { turn_id } => (None, None, Some(turn_id.clone())),
        };
        Self {
            connector,
            address,
            inherited,
        }
    }
}

/// What the ledger answered for one turn.
#[derive(Debug)]
pub(crate) struct Ledger {
    /// The row the key holds, this statement's or an earlier one's.
    pub(crate) stored: Repeated,
    /// Whether THIS statement created the row.
    pub(crate) inserted: bool,
}

impl Turns {
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
    /// needing review. A lost wake notification leaves the committed turn
    /// available to every poll.
    pub async fn admit(&self, turn: IncomingTurn<'_>) -> Result<Admitted> {
        self.admit_under(turn, None).await
    }

    /// [`Turns::admit`] for a fleet that must be runnable: a new key on a
    /// fleet in any other status is refused `NotRunnable`, while a repeat is
    /// still answered.
    ///
    /// # Errors
    /// As [`Turns::admit`], and `NotRunnable` with the fleet's status.
    pub async fn admit_runnable(&self, turn: IncomingTurn<'_>) -> Result<Admitted> {
        self.admit_under(turn, Some(FleetStatus::Active)).await
    }

    async fn admit_under(
        &self,
        turn: IncomingTurn<'_>,
        runnable: Option<FleetStatus>,
    ) -> Result<Admitted> {
        let now = clock::now();
        let row_id = self.entropy.uuid7(now)?;
        // An unrepeatable producer is keyed on the row it is about to write,
        // so its unique index still holds and this call is still the only
        // one that can own it. Minted HERE and not by the caller: the
        // entropy is the ledger's, so a deployment cannot end up with two
        // sources that could disagree.
        let key = match turn.key {
            Key::Repeated(key) => key,
            Key::Unrepeatable => row_id.as_str(),
        };
        let digest = turn.payload_digest();
        let ledger = self
            .commit(&row_id, &turn, key, &digest, now, runnable)
            .await
            .inspect_err(|refused| count_refusal(refused, &turn))?;
        Ok(self.answered(ledger, &turn, &digest).await)
    }

    /// [`Turns::admit_runnable`] behind `guard`, in one transaction.
    ///
    /// The guard is proved first, on the transaction the insert then runs on,
    /// so what it locks stays locked until the commit: a run a reassignment took
    /// after its holder last proved it writes nothing. The deployment estimate
    /// is read before the transaction opens, as [`Turns::prepare`] reads
    /// it, so no lock is held across a sample. An `Ok` answer always commits,
    /// a repeat included, because the statement's conflict arm writes.
    ///
    /// # Errors
    /// As [`Turns::admit_runnable`]. A guard that does not hold is not an
    /// error: it answers `Ok(Err(Unheld))`.
    pub async fn admit_runnable_guarded(
        &self,
        turn: IncomingTurn<'_>,
        guard: &dyn Precondition,
    ) -> Result<core::result::Result<Admitted, Unheld>> {
        let now = clock::now();
        let row_id = self.entropy.uuid7(now)?;
        let key = match turn.key {
            Key::Repeated(key) => key,
            Key::Unrepeatable => row_id.as_str(),
        };
        let digest = turn.payload_digest();
        let estimate = self.deployment_estimate(now).await;
        let mut transaction = sqlx::Transaction::begin(self.database.acquire().await?, None)
            .await
            .map_err(query(CONTEXT_GUARDED))?;
        if !guard
            .holds(&mut transaction)
            .await
            .map_err(query(CONTEXT_GUARDED))?
        {
            return Ok(Err(Unheld));
        }
        let gates = Gates {
            estimate,
            runnable: Some(FleetStatus::Active),
        };
        let ledger = self
            .insert_on(&mut transaction, &row_id, &turn, key, &digest, gates)
            .await
            .inspect_err(|refused| count_refusal(refused, &turn))?;
        transaction.commit().await.map_err(query(CONTEXT_GUARDED))?;
        if ledger.inserted {
            self.ceiling.admitted();
        }
        Ok(Ok(self.answered(ledger, &turn, &digest).await))
    }

    /// What a committed ledger answer tells its producer, once the fleet's
    /// wake has rung.
    async fn answered(&self, ledger: Ledger, turn: &IncomingTurn<'_>, digest: &str) -> Admitted {
        let producer = turn.producer.as_str();
        let turn_id = ledger.stored.id.as_str();
        if ledger.stored.digest != digest && !turn.producer.is_caller_keyed() {
            // The key is the identity and the first payload stands. Logged
            // at warn because a body this daemon renders differently than it
            // did is a deploy that changed a handler, and somebody should
            // know a sender saw both shapes.
            let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
            {
                let (error_code, fleet_id, event) = (&code, &turn.fleet, &"turn_payload_drifted");
                tracing::warn!(error_code, producer, %fleet_id, turn_id, event);
            };
        }
        self.wake(&ledger.stored.fleet).await;
        metrics::admitted(if ledger.inserted {
            IncomingTurnOutcome::Accepted
        } else {
            IncomingTurnOutcome::Replayed
        });
        let replayed = !ledger.inserted;
        {
            let (fleet_id, event) = (&turn.fleet, &"turn_committed");
            tracing::debug!(producer, %fleet_id, turn_id, event, replayed);
        };
        Admitted {
            replayed,
            stored: ledger.stored,
        }
    }

    /// Commit acceptance under the deployment ceiling, in one autocommit statement.
    ///
    /// `now` is the instant the row id was minted at, and the one the
    /// deployment ceiling's sampling cadence is read against; the row's own
    /// stamps are the database's.
    async fn commit(
        &self,
        row_id: &Uuid7,
        turn: &IncomingTurn<'_>,
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
                turn,
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

/// Counts and logs an intake that recorded nothing.
///
/// Two refusals, kept apart on the counter and in the log: a spent budget is
/// the deployment doing what it was told, and a database that would not answer
/// is an incident.
fn count_refusal(refused: &Error, turn: &IncomingTurn<'_>) {
    // A missing or archived fleet is the caller's answer, not an intake
    // failure: the ledger's own statement now refuses both, where the route
    // once did before the ledger saw them.
    if refused.is_fleet_absent() || refused.not_runnable().is_some() {
        return;
    }
    let (outcome, event) = if refused.is_over_capacity() {
        (
            IncomingTurnOutcome::OverBudget,
            "turn_refused_over_capacity",
        )
    } else {
        (IncomingTurnOutcome::Refused, "turn_refused")
    };
    metrics::admitted(outcome);
    let Logged { error_code, reason } = refused.logged();
    let producer = turn.producer.as_str();
    {
        let fleet_id = &turn.fleet;
        tracing::warn!(error_code, producer, %fleet_id, reason, event);
    };
}
