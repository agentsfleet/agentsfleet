//! The per-run renewal's metering statement: it measures the slice since the
//! run's cursor and extends the run. The report and the host batch carry
//! their own measurement inside their statements.

use af_core::id::Uuid7;
use afd_db::statement::Bound;

/// Measure the run time since the run's cursor, then extend the run.
///
/// `probe` reads the run and the thread under `FOR UPDATE OF l, a`; that thread
/// lock is what serialises a racing reassignment behind this statement. The run
/// time is measured off the RUN's own cursor, `last_metered_at`, on the
/// database's clock, and never past the deadline the run was granted: a
/// renewal that lands after its run lapsed bills no time beyond it, and a
/// reassigned run bills only from its own start. Tokens are not measured here:
/// the runner reports one line per provider request, and the caller logs them.
///
/// `guard` is the fence, and every write is `FROM guard`, so a superseded
/// holder writes nothing rather than some prefix of the writes. A renewal must
/// still be the live holder, before both deadlines. It grants
/// `capped = now + RUN_TTL` at any age, shortened only by a durable task's own
/// limits; the fleet's budget and a person's Stop end a run, not a clock.
///
/// `moved` extends the run to `capped` and advances its cursor; `moved_thread`
/// extends the thread's deadline the same way. Both deadline rows move together
/// because the assignment checks the thread while the runner stops its child on the
/// run: advancing one alone gets a healthy run reassigned.
///
/// The statement moves no money. Its trailing row answers its caller:
/// `moved_runs` and `new_until` for the verdict, and the measured
/// slice (`d_ms`, the instant it ends, who pays and what the line names) for
/// the runtime line the caller prices and logs in the same transaction.
/// `runtime_seq` numbers the runtime line by its slice's start, read from the
/// run row the statement locks: a statement that waits on `FOR UPDATE`
/// re-reads that row, so it sees the cursor a racing renewal or report
/// advanced, and the two lines take distinct numbers. The billing sweep
/// bills only a run past its deadline that has not reported, which a renewal
/// cannot extend, and a report behind it re-reads the advanced cursor and
/// measures no run time, so logs no runtime line.
///
/// `$1` run, `$2` runner, `$3` milliseconds per second, `$4` the active
/// status, `$5` the run lifetime a renewal grants, `$6` the running state a
/// task must hold. The metering instant is
/// `clock_timestamp()`, read once the rows are locked.
pub const METER_RUN: &str = include_str!("meter_run.sql");

/// What one metering pass measures: the run, the runner holding it and the
/// provider requests it reports.
#[derive(Debug, Clone, Copy)]
pub struct Metering<'a> {
    /// The run being metered.
    pub run_id: &'a Uuid7,
    /// The runner that holds it.
    pub runner_id: &'a Uuid7,
    /// One line per provider request since the last acknowledged renewal.
    pub requests: &'a [af_wire::usage::RequestUsage],
}

impl<'a> Metering<'a> {
    /// [`METER_RUN`] with every parameter bound, in `$n` order.
    pub(crate) fn bind(&self, clock: af_core::timing::RunClock) -> Bound<'a> {
        sqlx::query(METER_RUN)
            .bind(self.run_id)
            .bind(self.runner_id)
            .bind(afd_billing::nanos::MS_PER_SEC)
            .bind(super::RUN_STATUS_ACTIVE)
            .bind(clock.ttl())
            .bind(af_wire::task::TaskState::Running.as_str())
    }
}
