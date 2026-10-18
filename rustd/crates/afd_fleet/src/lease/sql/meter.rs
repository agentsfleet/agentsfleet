//! The one metering statement: a renewal and a settle charge the same slice
//! the same way and differ only in how the lease ends the pass.

use afd_billing::Meter;
use afd_core::id::Uuid7;
use afd_db::statement::Bound;

/// Meter the slice since the slot's cursor, then extend the lease or settle it.
///
/// `probe` reads the lease and the slot under `FOR UPDATE OF l, a`; that slot
/// lock is what serialises a racing reclaim behind this statement. The deltas
/// are measured off the SLOT's cursor rather than the lease's: the slot
/// survives a reclaim, so a re-leased run meters forward from where the dead
/// holder stopped, and a re-sent request charges about zero because the
/// previous one already advanced the cursor it is diffed against. Each delta
/// is clamped at zero, so counts below the cursor charge nothing.
///
/// `bal` locks the wallet in a common table expression (CTE) of its own:
/// Postgres refuses `FOR UPDATE` on the nullable side of an outer join, and a
/// tenant with no wallet still meters. `calc` prices the deltas at the bound
/// rates.
///
/// `guard` is the fence, and every write is `FROM guard`, so a superseded
/// holder writes nothing rather than some prefix of the writes. A renewal must
/// still be the live holder, before both deadlines, with
/// `capped = LEAST(now + LEASE_TTL, created_at + MAX_RUNTIME)` still ahead; a
/// run at its hard ceiling writes nothing and is not charged for asking. A
/// settle needs only that no reclaim superseded it, because a report may land
/// after its lease lapsed. `charged = LEAST(slice, bal0)` is the wallet's real
/// delta in every interleaving.
///
/// `moved` extends the lease to `capped` or flips it to reported; `slot`
/// extends the slot the same way and advances its cursor. Both deadline rows
/// move together because the claim checks the slot while the runner stops its
/// child on the lease: advancing one alone gets a healthy run reclaimed. Every
/// cursor clamps with `GREATEST(old, $n)`, so regressed cumulatives cannot
/// rewind it.
///
/// `wallet` drains `slice`, floored at zero. `ledger`'s `ON CONFLICT (event_id,
/// charge_type, fleet_id) DO UPDATE` accumulates every pass of one run into
/// its one `stage` row. `tally` counts only settles that flipped a lease.
///
/// The trailing row answers both callers: `claimed` and `charged_nanos` for a
/// settle; `new_until` (renewed only when `slot_updated` is one) and
/// `hard_cap` against the database's instant for a renewal.
///
/// `$1` lease, `$2` runner, `$3`–`$5` cumulative tokens, `$6`–`$9` the four
/// rates, `$10` charge type, `$11` milliseconds per second, `$12` tokens per
/// million, `$13` charge row id, `$14` the active status, `$15` whether this
/// settles, `$16` the lease lifetime a renewal grants, `$17` the hard cap on a
/// run, `$18` the reported status, `$19` whether the run succeeded. The
/// metering instant is `clock_timestamp()`, read once the rows are locked.
pub const METER_LEASE: &str = include_str!("meter_lease.sql");

/// How a metering pass ends the lease's current slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ending {
    /// Extend both deadlines, capped at the run's hard ceiling.
    Renew,
    /// Flip the lease to reported and count the run's outcome.
    Settle {
        /// Whether the run succeeded, which lifetime counter it moves.
        succeeded: bool,
    },
}

/// What one metering pass charges for: the lease, its runner, the cumulative
/// counts and rates the runner reported, and the charge row's id.
///
/// Seven of the nineteen parameters are same-typed integers; they arrive as
/// one [`Meter`] so a transposition has to get past a named type, and the `$n`
/// order is written once, in `Metering::bind`.
#[derive(Debug, Clone, Copy)]
pub struct Metering<'a> {
    /// The lease being metered.
    pub lease_id: &'a str,
    /// The runner that holds it.
    pub runner_id: &'a Uuid7,
    /// The cumulative counts and rates the runner reported.
    pub meter: Meter,
    /// The identifier a first charge row for this event takes.
    pub ledger_id: &'a Uuid7,
}

impl<'a> Metering<'a> {
    /// [`METER_LEASE`] with every parameter bound, in `$n` order.
    pub(crate) fn bind(&self, ending: Ending) -> Bound<'a> {
        let counts = self.meter.cumulative;
        let rates = self.meter.rates;
        let (settles, succeeded) = match ending {
            Ending::Renew => (false, false),
            Ending::Settle { succeeded } => (true, succeeded),
        };
        sqlx::query(METER_LEASE)
            .bind(self.lease_id)
            .bind(self.runner_id.as_str())
            .bind(counts.input)
            .bind(counts.cached)
            .bind(counts.output)
            .bind(rates.run_nanos_per_sec)
            .bind(rates.input_nanos_per_mtok)
            .bind(rates.cached_input_nanos_per_mtok)
            .bind(rates.output_nanos_per_mtok)
            .bind(afd_billing::sql::charge::STAGE)
            .bind(afd_billing::nanos::MS_PER_SEC)
            .bind(afd_billing::nanos::TOKENS_PER_MTOK)
            .bind(self.ledger_id.as_str())
            .bind(super::LEASE_STATUS_ACTIVE)
            .bind(settles)
            .bind(afd_core::timing::LEASE_TTL)
            .bind(afd_core::timing::MAX_RUNTIME)
            .bind(super::LEASE_STATUS_REPORTED)
            .bind(succeeded)
    }
}
