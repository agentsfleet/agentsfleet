//! The one metering statement: a renewal and a settle measure the same slice
//! the same way and differ only in how the lease ends the pass.

use afd_billing::Cumulative;
use afd_core::id::Uuid7;
use afd_db::statement::Bound;

/// Measure the slice since the slot's cursor, then extend the lease or settle it.
///
/// `probe` reads the lease and the slot under `FOR UPDATE OF l, a`; that slot
/// lock is what serialises a racing reclaim behind this statement. The deltas
/// are measured off the SLOT's cursor rather than the lease's: the slot
/// survives a reclaim, so a re-leased run meters forward from where the dead
/// holder stopped, and a re-sent request measures about zero because the
/// previous one already advanced the cursor it is diffed against. Each delta
/// is clamped at zero, so counts below the cursor measure nothing.
///
/// `guard` is the fence, and every write is `FROM guard`, so a superseded
/// holder writes nothing rather than some prefix of the writes. A renewal must
/// still be the live holder, before both deadlines, with
/// `capped = LEAST(now + LEASE_TTL, created_at + MAX_RUNTIME)` still ahead; a
/// run at its hard ceiling writes nothing and is not charged for asking. A
/// settle needs only that no reclaim superseded it, because a report may land
/// after its lease lapsed.
///
/// `moved` extends the lease to `capped` or flips it to reported; `slot`
/// extends the slot the same way and advances its cursor. Both deadline rows
/// move together because the claim checks the slot while the runner stops its
/// child on the lease: advancing one alone gets a healthy run reclaimed. Every
/// cursor clamps with `GREATEST(old, $n)`, so regressed cumulatives cannot
/// rewind it. `tally` counts only settles that flipped a lease.
///
/// The statement moves no money. Its trailing row answers both callers —
/// `claimed`, `new_until` and `hard_cap` for the verdict, and the measured
/// slice (`d_ms`, the three token deltas, the instant, who pays and what the
/// line names) for the usage line the caller prices and logs in the same
/// transaction. `logged_lines` is the lease's next line number: read under the
/// lease's row lock, every pass that could add a line for this lease waits on
/// that lock, so two passes can never take the same number.
///
/// `$1` lease, `$2` runner, `$3`–`$5` cumulative tokens, `$6` milliseconds per
/// second, `$7` the active status, `$8` whether this settles, `$9` the lease
/// lifetime a renewal grants, `$10` the hard cap on a run, `$11` the reported
/// status, `$12` whether the run succeeded, `$13` the usage lines' work type.
/// The metering instant is `clock_timestamp()`, read once the rows are locked.
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

/// What one metering pass measures: the lease, its runner and the cumulative
/// counts the runner reported.
#[derive(Debug, Clone, Copy)]
pub struct Metering<'a> {
    /// The lease being metered.
    pub lease_id: &'a str,
    /// The runner that holds it.
    pub runner_id: &'a Uuid7,
    /// The cumulative counts the runner reported.
    pub cumulative: Cumulative,
}

impl<'a> Metering<'a> {
    /// [`METER_LEASE`] with every parameter bound, in `$n` order.
    pub(crate) fn bind(&self, ending: Ending) -> Bound<'a> {
        let counts = self.cumulative;
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
            .bind(afd_billing::nanos::MS_PER_SEC)
            .bind(super::LEASE_STATUS_ACTIVE)
            .bind(settles)
            .bind(afd_core::timing::LEASE_TTL)
            .bind(afd_core::timing::MAX_RUNTIME)
            .bind(super::LEASE_STATUS_REPORTED)
            .bind(succeeded)
            .bind(afd_billing::sql::work::FLEET_LEASE)
    }
}
