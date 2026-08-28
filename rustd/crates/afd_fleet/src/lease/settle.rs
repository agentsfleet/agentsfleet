//! The report's money half: the lease it is about, and the one statement that
//! claims it and charges its final slice.
//!
//! Split from [`super::report`] the way [`super::affinity`] is split from
//! [`super::pull`] — this file is what the datastore does, that one is the
//! order it is asked to do it in.
//!
//! # `Settled` is an enum where the Zig has a struct
//!
//! `renewal_settle.zig` answers `{ claimed: bool, charged_nanos: i64 }`, and
//! the two fields are not independent: a report that lost the fence charges
//! nothing, so `claimed = false` with a non-zero amount is a state the type
//! permits and the code cannot produce. Ported literally that leaves every
//! caller free to read the amount without checking the flag — which is exactly
//! the bug the flag exists to prevent, one `if` away at every call site.
//!
//! Rust can say it once: [`Settled::Claimed`] CARRIES the amount and
//! [`Settled::Fenced`] has nowhere to put one. Reading a charge without having
//! established the claim is not a mistake to avoid; it does not compile.

use afd_core::clock::UnixMillis;
use afd_core::id::{ENTROPY_LEN, Uuid7};
use sqlx::Row as _;

use crate::error::{Result, query, row_malformed};
use crate::lease::affinity::Fence;
use crate::lease::store::Leases;
use crate::sql;
use crate::sql::report::SettleRow;
use afd_billing::{Meter, Nanos};

/// Statement name, for the context a query failure carries.
const CONTEXT_LOAD: &str = "report lease load";

/// Statement name, for the context a query failure carries.
const CONTEXT_SETTLE: &str = "report claim and settle";

/// The table a malformed identifier column is reported against.
const TABLE_LEASES: &str = "fleet.runner_leases";

/// The lease a report is about, as the row holds it.
///
/// Every identifier is a [`Uuid7`] rather than the `[]const u8` the Zig
/// arena-dups: the columns are `uuid` and the finalize path passes them to four
/// more statements, so parsing once here is what stops a malformed one reaching
/// a `::uuid` cast four frames down and failing as a query error instead of as
/// the data fault it is.
#[derive(Debug, Clone)]
pub struct Reported {
    /// The fleet whose slot this lease holds.
    pub fleet_id: Uuid7,
    /// The workspace the work belongs to.
    pub workspace_id: Uuid7,
    /// The tenant whose wallet the settle draws on.
    pub tenant_id: Uuid7,
    /// The event that was executed.
    pub event_id: String,
    /// Who or what raised the event.
    pub actor: String,
    /// The billing posture resolved at issue.
    pub posture: String,
    /// The provider resolved at issue.
    pub provider: String,
    /// The model resolved at issue.
    pub model: String,
    /// The token this lease was issued under — what the settle is fenced on.
    pub fence: Fence,
}

/// What the claim-and-settle statement decided.
///
/// Two variants, and the absent third is the point: there is no "claimed but
/// unpriced" state, because the guard that admits the claim is the same guard
/// the charge rides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Settled {
    /// This holder won the report. The final slice drained this much — which
    /// may legitimately be zero, when the run's last window was short enough
    /// or the wallet was already empty.
    Claimed(Nanos),
    /// A newer holder has the fleet. Nothing was written and nothing charged.
    Fenced,
}

impl Leases {
    /// The lease `lease_id` names, if it belongs to `runner_id`.
    ///
    /// `Ok(None)` for both "no such lease" and "somebody else's lease" — the
    /// statement's `runner_id` predicate cannot tell them apart, which is
    /// deliberate. See [`sql::report::SELECT_LEASE_FOR_REPORT`].
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a row whose identifier
    /// columns are not identifiers.
    pub async fn load_for_report(
        &self,
        lease_id: &str,
        runner_id: &Uuid7,
    ) -> Result<Option<Reported>> {
        let mut connection = self.pool().acquire().await?;
        let found = sqlx::query(sql::report::SELECT_LEASE_FOR_REPORT)
            .bind(lease_id)
            .bind(runner_id.as_str())
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_LOAD))?;

        let Some(row) = found else {
            return Ok(None);
        };
        let text = |index: usize| -> Result<String> {
            row.try_get::<String, _>(index).map_err(query(CONTEXT_LOAD))
        };
        let id = |index: usize, column: &'static str| -> Result<Uuid7> {
            Uuid7::parse(&text(index)?).map_err(row_malformed(TABLE_LEASES, column))
        };
        let fence: i64 = row.try_get(8).map_err(query(CONTEXT_LOAD))?;
        Ok(Some(Reported {
            fleet_id: id(0, "fleet_id")?,
            workspace_id: id(1, "workspace_id")?,
            tenant_id: id(2, "tenant_id")?,
            event_id: text(3)?,
            actor: text(4)?,
            posture: text(5)?,
            provider: text(6)?,
            model: text(7)?,
            fence: Fence::from_i64(fence),
        }))
    }

    /// Flip the lease to `reported` and charge its final slice, atomically.
    ///
    /// The fence check, the flip, both metering cursors, the wallet drain, the
    /// ledger row and the lifetime tally ride ONE statement. A superseded
    /// holder writes none of them — see [`sql::report::CLAIM_AND_SETTLE`] for
    /// why fusing them is what makes the cap path safe.
    ///
    /// # Errors
    /// Reports an entropy source that could not produce the ledger row's
    /// identifier, an instant that cannot be encoded, and a datastore that
    /// would not answer. A LOST fence is [`Settled::Fenced`], not an error:
    /// nothing failed, and the caller owes the runner a refusal rather than a
    /// retry.
    pub async fn claim_and_settle(
        &self,
        lease_id: &str,
        runner_id: &Uuid7,
        meter: Meter,
        succeeded: bool,
        now: UnixMillis,
    ) -> Result<Settled> {
        let mut bytes = [0u8; ENTROPY_LEN];
        self.entropy().fill(&mut bytes)?;
        let ledger_id = Uuid7::encode(now, bytes)?;

        let settle = SettleRow {
            lease_id,
            runner_id,
            now,
            meter,
            ledger_id: &ledger_id,
            succeeded,
        };
        let mut connection = self.pool().acquire().await?;
        let row = settle
            .bind()
            .fetch_one(&mut *connection)
            .await
            .map_err(query(CONTEXT_SETTLE))?;

        // `charged` is nullable because the guard may yield no row at all; the
        // claim count is the authority on whether this holder won, and the
        // amount is only read on the arm where it did.
        let claimed: i64 = row.try_get(1).map_err(query(CONTEXT_SETTLE))?;
        if claimed == 0 {
            return Ok(Settled::Fenced);
        }
        let charged: Option<i64> = row.try_get(0).map_err(query(CONTEXT_SETTLE))?;
        // A surviving guard row always prices a charge, so a null here would
        // mean the claim flipped without `calc` — read as zero drain rather
        // than reporting a debit the wallet never took.
        Ok(Settled::Claimed(Nanos::from_i64(charged.unwrap_or(0))))
    }
}

#[cfg(test)]
mod tests {
    use super::Settled;
    use afd_billing::Nanos;

    /// A fenced settle has nowhere to carry a charge.
    ///
    /// The property the enum exists for, asserted as the shape rather than as a
    /// value: `Settled::Fenced` takes no payload, so no caller can read an
    /// amount off a report that lost the fence. The Zig's paired
    /// `{ claimed, charged_nanos }` permits exactly that and relies on every
    /// call site to check the flag first.
    #[test]
    fn test_only_a_claimed_settle_carries_a_charge() {
        let claimed = Settled::Claimed(Nanos::from_i64(42));
        assert!(
            matches!(claimed, Settled::Claimed(nanos) if nanos.as_i64() == 42),
            "a won claim reports what it drained"
        );
        assert_ne!(
            Settled::Fenced,
            Settled::Claimed(Nanos::ZERO),
            "a fenced report is not a claim that happened to charge nothing"
        );
    }
}
