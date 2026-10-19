//! The report's money half: the lease it is about, and the one statement that
//! claims it and measures its final slice.
//!
//! Split from [`super::report`] the way [`super::affinity`] is split from
//! [`super::pull`] — this file is what the datastore does, that one is the
//! order it is asked to do it in.
//!
//! # `Settled` is an enum
//!
//! Whether the claim won and what it logged are not independent: a report that
//! lost the fence logs nothing. A `{ claimed: bool, line: Option<..> }` pair
//! would permit `claimed = false` with a line and leave every caller free to
//! read the line without checking the flag, one `if` away at every call site.
//!
//! The enum says it once: [`Settled::Claimed`] CARRIES the final slice's usage
//! line and neither [`Settled::Fenced`] nor [`Settled::AlreadySettled`] has
//! anywhere to put one. Reading a charge without having established the claim
//! is not a mistake to avoid; it does not compile.

use afd_core::id::Uuid7;
use jiff::Timestamp;
use sqlx::{FromRow as _, PgConnection};

use crate::error::{Result, query, row_malformed};
use crate::lease::affinity::Fence;
use crate::lease::sql;
use crate::lease::sql::meter::{Ending, Metering};
use crate::lease::store::Leases;
use crate::lease::usage::{self, Pricing, SliceColumns};
use afd_billing::biller::Settlement;

/// Statement name, for the context a query failure carries.
const CONTEXT_LOAD: &str = "report lease load";

/// Statement name, for the context a query failure carries.
const CONTEXT_SETTLE: &str = "report claim and settle";

/// Statement name, for the context a query failure carries.
const CONTEXT_DISPOSITION: &str = "report lease disposition";

/// The table a malformed identifier column is reported against.
const TABLE_LEASES: &str = "runner.leases";

/// The lease a report is about, as the row holds it.
///
/// Every identifier is a [`Uuid7`] rather than a string: the columns are
/// `uuid` and the finalize path passes them to four
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
    /// The event that was executed — the ledger's logical id.
    pub event_id: String,
    /// The stream entry it arrived on, which is what the acknowledgement
    /// addresses.
    pub receipt: String,
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
/// Three dispositions, and the absent fourth is the point: there is no
/// "claimed but unpriced" state, because the guard that admits the claim is
/// the same guard the charge rides.
///
/// # Why an empty claim is two answers, not one
///
/// The statement guards on `status = active`, so it writes nothing both for a
/// holder the fleet superseded and for this same runner re-sending a report it
/// already settled. Collapsed into one refusal they behave identically and are
/// opposite facts: the first must be refused, and the second must be told what
/// it already achieved. A runner that loses the response to a terminal report
/// and retries into a permanent refusal has thrown a finished run's answer
/// away — RULE IDMP, and the reason [`Settled::AlreadySettled`] exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Settled {
    /// This holder won the report. Carries the final slice's usage line,
    /// logged in the report's transaction and still to be settled; `None` when
    /// the last window measured nothing.
    Claimed(Option<Settlement>),
    /// This runner already settled this lease. Nothing was written and nothing
    /// logged, because the first report wrote and logged it; the outcome the
    /// caller owes its runner is the one already durable.
    AlreadySettled,
    /// A newer holder has the fleet. Nothing was written and nothing logged.
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
        let found: Option<LeaseForReport> = sqlx::query_as(sql::report::SELECT_LEASE_FOR_REPORT)
            .bind(lease_id)
            .bind(runner_id.as_str())
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_LOAD))?;
        found.map(LeaseForReport::into_reported).transpose()
    }

    /// Flip the lease to `reported`, measure its final slice and log that
    /// slice's usage line, atomically.
    ///
    /// The fence check, the flip, both metering cursors and the lifetime tally
    /// ride ONE statement, and the line is logged right behind it on the same
    /// connection. A superseded holder writes none of them — see
    /// [`sql::meter::METER_LEASE`].
    ///
    /// Runs on a connection the CALLER owns, because the line is one write in
    /// a transaction that carries several: the result, the checkpoint and the
    /// freed slot commit with it or not at all
    /// ([`Leases::commit_report`](crate::lease::commit)). Acquiring a second
    /// connection here would put the line outside that boundary and hold two
    /// connections of a five-connection pool for one report (RULE CNX). Money
    /// moves after commit, through the caller's biller. The caller also holds
    /// the host's capacity lock first, which is what numbers this line after a
    /// racing renewal's (see [`sql::meter::METER_LEASE`]).
    ///
    /// # Errors
    /// Reports an entropy source that could not produce the line's
    /// identifier, an instant that cannot be encoded, a charge past a 64-bit
    /// nano count, and a datastore that would not answer. A LOST fence is
    /// [`Settled::Fenced`] and an already-settled lease is
    /// [`Settled::AlreadySettled`], neither an error: nothing failed, and what
    /// the caller owes the runner differs between them.
    pub async fn claim_and_settle(
        &self,
        connection: &mut PgConnection,
        lease: Metering<'_>,
        lease_key: &Uuid7,
        pricing: &Pricing,
        succeeded: bool,
        now: Timestamp,
    ) -> Result<Settled> {
        let row = lease
            .bind(Ending::Settle { succeeded })
            .fetch_one(&mut *connection)
            .await
            .map_err(query(CONTEXT_SETTLE))?;
        let answer = SettleAnswer::from_row(&row).map_err(query(CONTEXT_SETTLE))?;

        // The claim count is the authority on whether this holder won; the
        // slice is only read on the arm where it did.
        if answer.claimed == 0 {
            return disposition(connection, lease.lease_id, lease.runner_id).await;
        }
        let slice = SliceColumns::from_row(&row).map_err(query(CONTEXT_SETTLE))?;
        // A won claim is the guard row matching, and the slice is read off that
        // same row, so it is always present here.
        let settlement = slice.into_slice().map_or(Ok(None), |slice| {
            pricing.settlement(lease_key, slice, lease.requests)
        })?;
        usage::log_measured(connection, self.entropy(), now, settlement.as_ref()).await?;
        Ok(Settled::Claimed(settlement))
    }
}

/// Why the claim matched nothing, decided on the claim's own snapshot.
///
/// A free function rather than a method: it needs the connection and the two
/// identifiers and nothing the store owns, which is the shape
/// `afd_approval`'s `holds` already uses for the same reason.
///
/// A row that is gone answers [`Settled::Fenced`] — the lease was loaded a
/// moment ago and is not here now, so there is nothing left to report against
/// and nothing to hand back.
async fn disposition(
    connection: &mut PgConnection,
    lease_id: &str,
    runner_id: &Uuid7,
) -> Result<Settled> {
    let status: Option<String> = sqlx::query_scalar(sql::report::SELECT_LEASE_DISPOSITION)
        .bind(lease_id)
        .bind(runner_id.as_str())
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(CONTEXT_DISPOSITION))?;
    Ok(if status.as_deref() == Some(sql::LEASE_STATUS_REPORTED) {
        Settled::AlreadySettled
    } else {
        Settled::Fenced
    })
}

#[cfg(test)]
mod tests {
    use super::Settled;

    /// Only a won claim carries a usage line, and the two empty claims differ.
    ///
    /// The property the enum exists for, asserted as the shape rather than as a
    /// value: neither `Settled::Fenced` nor `Settled::AlreadySettled` takes a
    /// payload, so no caller can read a line off a report that logged none.
    /// A superseded holder and a runner re-sending a settled report both write
    /// nothing, and collapsing them into one value is how a finished run's
    /// answer gets refused forever.
    #[test]
    fn test_only_a_claimed_settle_carries_a_line() {
        assert_ne!(
            Settled::Fenced,
            Settled::Claimed(None),
            "a fenced report is not a claim that happened to measure nothing"
        );
        assert_ne!(
            Settled::AlreadySettled,
            Settled::Fenced,
            "a report already settled by this runner is not a report it lost"
        );
    }
}

/// The column of [`sql::meter::METER_LEASE`]'s answer a settle reads.
#[derive(sqlx::FromRow)]
struct SettleAnswer {
    claimed: i64,
}

/// [`sql::report::SELECT_LEASE_FOR_REPORT`]'s row, ids still as stored text.
#[derive(sqlx::FromRow)]
struct LeaseForReport {
    fleet_id: String,
    workspace_id: String,
    tenant_id: String,
    event_id: String,
    actor: String,
    posture: String,
    provider: String,
    model: String,
    fencing_token: i64,
    receipt: String,
}

impl LeaseForReport {
    fn into_reported(self) -> Result<Reported> {
        let id = |text: &str, column: &'static str| {
            Uuid7::parse(text).map_err(row_malformed(TABLE_LEASES, column))
        };
        Ok(Reported {
            fleet_id: id(&self.fleet_id, "fleet_id")?,
            workspace_id: id(&self.workspace_id, "workspace_id")?,
            tenant_id: id(&self.tenant_id, "tenant_id")?,
            event_id: self.event_id,
            receipt: self.receipt,
            actor: self.actor,
            posture: self.posture,
            provider: self.provider,
            model: self.model,
            fence: Fence::from_i64(self.fencing_token),
        })
    }
}
