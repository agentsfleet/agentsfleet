//! The per-lease renewal's metering statement: it measures the slice since the
//! lease's cursor and extends the lease. The report and the host batch carry
//! their own measurement inside their statements.

use af_core::id::Uuid7;
use afd_db::statement::Bound;

/// Measure the run time since the lease's cursor, then extend the lease.
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
/// run at its hard ceiling writes nothing and is not charged for asking.
///
/// `moved` extends the lease to `capped` and advances its cursor; `slot`
/// extends the slot's deadline the same way. Both deadline rows move together
/// because the claim checks the slot while the runner stops its child on the
/// lease: advancing one alone gets a healthy run reclaimed.
///
/// The statement moves no money. Its trailing row answers its caller:
/// `claimed`, `new_until` and `hard_cap` for the verdict, and the measured
/// slice (`d_ms`, the instant it ends, who pays and what the line names) for
/// the runtime line the caller prices and logs in the same transaction.
/// `runtime_seq` numbers the runtime line by its slice's start, read from the
/// lease row the statement locks: a statement that waits on `FOR UPDATE`
/// re-reads that row, so it sees the cursor a racing renewal or report
/// advanced, and the two lines take distinct numbers (M245). The billing sweep
/// bills only a lease past its deadline that has not reported, which a renewal
/// cannot extend, and a report behind it re-reads the advanced cursor and
/// measures no run time, so logs no runtime line.
///
/// `$1` lease, `$2` runner, `$3` milliseconds per second, `$4` the active
/// status, `$5` the lease lifetime a renewal grants, `$6` the hard cap on a
/// run, `$7` the running state a task must hold. The metering instant is
/// `clock_timestamp()`, read once the rows are locked.
pub const METER_LEASE: &str = include_str!("meter_lease.sql");

/// What one metering pass measures: the lease, the runner holding it and the
/// provider requests it reports.
#[derive(Debug, Clone, Copy)]
pub struct Metering<'a> {
    /// The lease being metered.
    pub lease_id: &'a Uuid7,
    /// The runner that holds it.
    pub runner_id: &'a Uuid7,
    /// One line per provider request since the last acknowledged renewal.
    pub requests: &'a [af_wire::usage::RequestUsage],
}

impl<'a> Metering<'a> {
    /// [`METER_LEASE`] with every parameter bound, in `$n` order.
    pub(crate) fn bind(&self) -> Bound<'a> {
        sqlx::query(METER_LEASE)
            .bind(self.lease_id)
            .bind(self.runner_id)
            .bind(afd_billing::nanos::MS_PER_SEC)
            .bind(super::LEASE_STATUS_ACTIVE)
            .bind(af_core::timing::LEASE_TTL)
            .bind(af_core::timing::MAX_RUNTIME)
            .bind(af_wire::task::TaskState::Running.as_str())
    }
}
