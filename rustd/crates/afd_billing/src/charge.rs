//! Recording what a tenant was charged.
//!
//! # The row is the point, even when the amount is zero
//!
//! `computeReceiveCharge` returns zero under both postures today, which makes
//! the receive debit look like a no-op worth skipping. It is not. The ROW is
//! what [`Accounts::spend`] apportions against a ceiling and what the charges
//! endpoint renders, so a charge of zero that is RECORDED is a different thing
//! from a charge that never happened — and only one of the two can later be
//! priced without a migration.
//!
//! # Replay safety, and the half of it that is not in the statement
//!
//! `ON CONFLICT (event_id, charge_type) DO NOTHING` makes the ledger row
//! idempotent. It does NOT make the balance drain idempotent, because the drain
//! is a different write. So the statement protects the row and the CALLER
//! protects the money, by charging on a first delivery only — see
//! [`Accounts::debit_receive`].

use afd_core::clock::UnixMillis;
use afd_core::id::{ENTROPY_LEN, Uuid7};

use crate::error::{Result, query};
use crate::rates::Posture;
use crate::sql;
use crate::store::Accounts;
use crate::{Nanos, RECEIVE_NANOS};

/// Statement name, for the context a query failure carries.
const CONTEXT_CHARGE: &str = "usage ledger insert";

/// A charge was recorded.
///
/// `LOGGING_STANDARD.md` §3 `event` value, spelled as `metering.zig` spells it.
const EVENT_DEBIT: &str = "debit";

/// Who and what a charge is recorded against.
///
/// One struct rather than nine positional parameters, for the reason
/// [`afd_fleet::sql::lease::LeaseRow`] is one: the insert binds sixteen values and
/// four of them are identifiers of the same shape, which compile clean in any
/// order. `metering.zig`'s `PreflightContext` groups the same fields for the
/// same reason and then passes `tenant_id` alongside it rather than inside it,
/// which is the one thing not copied here — the tenant is who is charged, so it
/// belongs with the rest of the identity.
#[derive(Debug, Clone, Copy)]
pub struct Charged<'a> {
    /// The wallet the charge is drawn against.
    pub tenant_id: &'a Uuid7,
    /// The workspace the work belongs to.
    pub workspace_id: &'a Uuid7,
    /// The fleet whose ceiling this counts against.
    pub fleet_id: &'a Uuid7,
    /// The event being charged for.
    pub event_id: &'a str,
    /// Who supplied the provider key.
    pub posture: Posture,
    /// The model the run will use.
    pub model: &'a str,
    /// The event's own creation instant.
    ///
    /// The EVENT's, never a clock read here: every ledger row for one event
    /// must carry the same value, and the receive row is written on a different
    /// path at a different moment from the stage row a renewal accumulates. Two
    /// clock reads straddling a millisecond would disagree.
    pub event_created_at: UnixMillis,
}

impl Accounts {
    /// Record the receive charge for an event.
    ///
    /// **Call this on a first delivery only.** The caller decides that by
    /// matching on [`Delivery`](crate::lease::event::Delivery), which is a
    /// two-variant enum and therefore an exhaustive match — there is no
    /// third state to fall through and no bool to invert. Re-stating the rule
    /// here as a witness argument was tried and removed: the match and this
    /// call sit on adjacent lines in one function, so the guarantee has no
    /// distance to travel and the extra type bought nothing the compiler was
    /// not already enforcing.
    ///
    /// It matters because the ledger row is replay-guarded and the BALANCE
    /// drain is not, so a redelivery charged twice leaves one row to show for
    /// two charges.
    ///
    /// Answers what was drained, rather than emitting a metric: the credit
    /// meter is §6/M181's, and fusing the two here is what makes
    /// `service_billing.zig` unable to run its money path without an exporter
    /// configured.
    ///
    /// # Errors
    /// Reports an entropy source that could not produce the row's identifier,
    /// an instant that cannot be encoded, and a datastore that would not
    /// answer. Every one of those leaves the delivery leasable — the caller
    /// answers no-work and the next poll retries.
    pub async fn debit_receive(&self, charged: Charged<'_>, now: UnixMillis) -> Result<Nanos> {
        self.record(charged, sql::charge::RECEIVE, RECEIVE_NANOS, now)
            .await?;

        // Hoisted: the `log` bridge duplicates field expressions and llvm-cov
        // scores the dead copy.
        let tenant = charged.tenant_id.as_str();
        let event = charged.event_id;
        let nanos = RECEIVE_NANOS.as_i64();
        tracing::debug!(
            event = EVENT_DEBIT,
            charge_type = sql::charge::RECEIVE,
            tenant_id = tenant,
            agentsfleet_event_id = event,
            nanos,
            "an event was admitted and its receive charge recorded"
        );
        Ok(RECEIVE_NANOS)
    }

    /// Write one `billing.usage_ledger` row.
    ///
    /// Split from the charge verbs above because both of this milestone's debit
    /// points land the same row shape and differ only in charge type and
    /// amount — and because binding sixteen parameters inside a verb would push
    /// it past the function-length line for no gain.
    ///
    /// No transaction. `metering.zig` wraps its insert in `BEGIN`/`COMMIT` with
    /// a `tx_open` flag and two rollback call sites, which was load-bearing
    /// when the balance drain and this row were written together. They are not:
    /// the drain lives in the renewal CTE, so this is a single statement, and a
    /// transaction around one statement is a round trip that buys nothing. The
    /// debit that DOES move a balance opens a real one.
    async fn record(
        &self,
        charged: Charged<'_>,
        charge_type: &str,
        nanos: Nanos,
        now: UnixMillis,
    ) -> Result<()> {
        let mut bytes = [0u8; ENTROPY_LEN];
        self.entropy().fill(&mut bytes)?;
        let row_id = Uuid7::encode(now, bytes)?;

        let mut connection = self.pool().acquire().await?;
        sqlx::query(sql::INSERT_USAGE_LEDGER)
            .bind(row_id.as_str())
            .bind(charged.tenant_id.as_str())
            .bind(charged.workspace_id.as_str())
            .bind(charged.fleet_id.as_str())
            .bind(charged.event_id)
            .bind(charge_type)
            .bind(charged.posture.as_str())
            .bind(charged.model)
            .bind(nanos.as_i64())
            // The four measurement columns are NULL on a charge recorded before
            // the run: nothing has been counted yet. The renewal CTE fills them
            // on the stage row as slices accumulate.
            .bind(Option::<i64>::None)
            .bind(Option::<i64>::None)
            .bind(Option::<i64>::None)
            .bind(Option::<i64>::None)
            .bind(charged.event_created_at.as_millis())
            .bind(now.as_millis())
            // A one-shot charge's span is a point, so the budget drain's
            // apportioning degenerates to all-or-nothing for this row.
            .bind(now.as_millis())
            .execute(&mut *connection)
            .await
            .map_err(query(CONTEXT_CHARGE))?;
        Ok(())
    }
}
