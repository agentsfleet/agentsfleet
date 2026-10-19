//! The one metering statement: a renewal and a settle measure the same slice
//! the same way and differ only in how the lease ends the pass.

use afd_core::id::Uuid7;
use afd_db::statement::Bound;

/// Measure the run time since the lease's cursor, then extend the lease or
/// settle it.
///
/// `probe` reads the lease and the slot under `FOR UPDATE OF l, a`; that slot
/// lock is what serialises a racing reclaim behind this statement. The run
/// time is measured off the LEASE's own cursor, `last_metered_at`, on the
/// database's clock, and never past the deadline the lease was granted: a
/// report that lands after its lease lapsed bills no time beyond it, and a
/// reclaimed run bills only from its own start. Tokens are not measured here:
/// the runner reports one line per provider request, and the caller logs them.
///
/// `guard` is the fence, and every write is `FROM guard`, so a superseded
/// holder writes nothing rather than some prefix of the writes. A renewal must
/// still be the live holder, before both deadlines, with
/// `capped = LEAST(now + LEASE_TTL, created_at + MAX_RUNTIME)` still ahead; a
/// run at its hard ceiling writes nothing and is not charged for asking. A
/// settle needs only that no reclaim superseded it.
///
/// `moved` extends the lease to `capped` or flips it to reported, and advances
/// its cursor; `slot` extends the slot's deadline the same way. Both deadline
/// rows move together because the claim checks the slot while the runner stops
/// its child on the lease: advancing one alone gets a healthy run reclaimed.
/// `tally` counts only settles that flipped a lease.
///
/// The statement moves no money. Its trailing row answers both callers:
/// `claimed`, `new_until` and `hard_cap` for the verdict, and the measured
/// slice (`d_ms`, the instant it ends, who pays and what the line names) for
/// the runtime line the caller prices and logs in the same transaction.
/// `daemon_lines` counts the runtime lines the lease already logged, and the
/// row lock does NOT make that count current: under READ COMMITTED a statement
/// that waits on `FOR UPDATE` re-reads the locked rows but keeps its starting
/// snapshot for the count. A renewal and a report therefore each take the
/// host's capacity lock (`afd_runner::capacity::lock`) in an earlier statement
/// of the same transaction, so this one starts after the other committed and
/// counts its line. The billing sweep needs no such lock: it bills only a
/// lease past its deadline that has not reported, which a renewal cannot
/// extend, and a report behind the sweep re-reads the advanced cursor and
/// measures no run time, so logs no runtime line.
///
/// `$1` lease, `$2` runner, `$3` milliseconds per second, `$4` the active
/// status, `$5` whether this settles, `$6` the lease lifetime a renewal grants,
/// `$7` the hard cap on a run, `$8` the reported status, `$9` whether the run
/// succeeded, `$10` the usage lines' work type, `$11` the first daemon line
/// number. The metering instant is `clock_timestamp()`, read once the rows are
/// locked.
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

/// What one metering pass measures: the lease, the runner holding it and the
/// provider requests it reports.
#[derive(Debug, Clone, Copy)]
pub struct Metering<'a> {
    /// The lease being metered.
    pub lease_id: &'a str,
    /// The runner that holds it.
    pub runner_id: &'a Uuid7,
    /// One line per provider request since the last acknowledged renewal.
    pub requests: &'a [afd_wire::usage::RequestUsage],
}

impl<'a> Metering<'a> {
    /// [`METER_LEASE`] with every parameter bound, in `$n` order.
    pub(crate) fn bind(&self, ending: Ending) -> Bound<'a> {
        let (settles, succeeded) = match ending {
            Ending::Renew => (false, false),
            Ending::Settle { succeeded } => (true, succeeded),
        };
        sqlx::query(METER_LEASE)
            .bind(self.lease_id)
            .bind(self.runner_id.as_str())
            .bind(afd_billing::nanos::MS_PER_SEC)
            .bind(super::LEASE_STATUS_ACTIVE)
            .bind(settles)
            .bind(afd_core::timing::LEASE_TTL)
            .bind(afd_core::timing::MAX_RUNTIME)
            .bind(super::LEASE_STATUS_REPORTED)
            .bind(succeeded)
            .bind(afd_billing::sql::work::FLEET_LEASE)
            .bind(i64::from(afd_wire::usage::DAEMON_SEQ_BASE))
    }
}
