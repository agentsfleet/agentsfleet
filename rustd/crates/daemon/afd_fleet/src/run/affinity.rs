//! `thread.threads` — one THREAD row per fleet: the atomic assignment, the
//! monotonic fencing source, and the sticky-routing hint, all on one row.
//!
//! The GUARANTEE is that exactly one of N racing runners wins a fleet, and the
//! winner carries a token that is strictly greater than every token issued
//! before it. "No thread won" is an absence and says so as [`Option::None`]
//! rather than as a second variant a caller must remember to match. The verbs
//! are methods on [`Runs`], which owns the pool and keeps it `pub(crate)`, so
//! nothing outside this crate can run a statement that is not in
//! [`crate::run::sql`]. They run the thread statements every kind shares
//! ([`crate::run::sql::shared`]); a fleet's thread is keyed by the fleet's id.
//!
//! # Why the token is a type
//!
//! A run row binds five `i64`s in a row — `turn_created_at`,
//! `fencing_token`, `run_expires_at`, and two instants — and two of them
//! transposed compiles clean and writes a run that can never be reported
//! against. [`Fence`] exists so that only the value an assignment minted can reach
//! the column that fences reports, and §3 can demand one by type rather than by
//! comment.

use af_core::error_code::{Coded as _, Logged};
use af_core::id::Uuid7;
use af_observability::metrics::label::fleet::HeldAssignment;
use af_observability::producers;
#[cfg(any(test, feature = "test-util"))]
use jiff::Timestamp;
#[cfg(any(test, feature = "test-util"))]
use sqlx::PgConnection;
#[cfg(any(test, feature = "test-util"))]
use std::time::Duration;

use crate::error::{Result, query};
use crate::run::envelope::Acquired;
#[cfg(any(test, feature = "test-util"))]
use crate::run::owner::RunOwner;
use crate::run::sql;
use crate::run::store::Runs;

#[cfg(any(test, feature = "test-util"))]
/// Statement name, for the context a query failure carries.
const CONTEXT_ASSIGN: &str = "affinity assignment";

/// Statement name, for the context a query failure carries.
const CONTEXT_RELEASE: &str = "affinity release";

/// An assignment that issued no run could not be freed; it lapses at its expiry.
pub(crate) const EVENT_ASSIGNMENT_RELEASE_FAILED: &str = "run_assignment_release_failed";

/// The `fencing_token` column, which is the one number that orders run holders.
///
/// Monotonic per fleet and minted by the thread upsert inside the assignment head
/// (`assign_turn_head.sql`): every winning assignment bumps it, so a holder
/// superseded by a reassignment carries a strictly smaller value than the runner
/// that displaced it. That is the whole basis of
/// Invariant 2, and the reason a report verifies its token inside the same
/// statement that flips the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Fence(i64);

impl Fence {
    /// The token as the column stores it.
    #[must_use]
    pub const fn as_i64(self) -> i64 {
        self.0
    }

    /// The token as the wire spells it.
    ///
    /// `fencing_token` is a monotonic counter the assignment statement starts at one
    /// and only ever increments, so no value this crate produces is negative.
    /// One edited out of band could be, and it saturates to ZERO rather than
    /// panicking or taking its magnitude: zero is below every token an assignment
    /// can mint, so a corrupted row fences itself out. `unsigned_abs` alone
    /// would turn `-1` into `1`, which is a token another holder may legitimately
    /// hold — a plausible wrong answer, which is the worse failure.
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        if self.0 < 0 { 0 } else { self.0.unsigned_abs() }
    }

    /// A token as read back from a row.
    ///
    /// `pub(crate)` on purpose: outside this crate a `Fence` can only come from
    /// an assignment or from a row this crate read, never from arithmetic on an
    /// `i64` a caller happened to have.
    pub(crate) const fn from_i64(value: i64) -> Self {
        Self(value)
    }
}

/// A won assignment: the new token, the instant the thread — and the run issued
/// against it — stays valid until, and whether the winner held the fleet.
///
/// What the test-only [`Runs::assign`] answers; production reads the same
/// facts from the assignment head's row.
#[cfg(any(test, feature = "test-util"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WonAssignment {
    /// The monotonic token this assignment minted.
    pub fence: Fence,
    /// When the assignment lapses and the thread becomes winnable again.
    pub held_until: Timestamp,
    /// Whether the winner holds the sandbox the fleet's last run left: that
    /// run ran on this runner and its hold had not lapsed at the assignment.
    pub resume_hold: bool,
}

#[cfg(any(test, feature = "test-util"))]
/// What [`sql::shared::ASSIGN_THREAD`] answers when the assignment wins: the new
/// fence, the thread's deadline on the database clock, and whether the thread's
/// prior hold was still live and whose it was.
#[derive(Debug, sqlx::FromRow)]
struct Won {
    fencing_token: i64,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    run_expires_at: Timestamp,
    held: bool,
    holder: Option<Uuid7>,
}

impl Runs {
    #[cfg(any(test, feature = "test-util"))]
    /// Atomically assign `fleet_id`'s thread to `runner_id`, valid for
    /// `ttl_ms`.
    ///
    /// Wins iff the thread is unassigned or its prior assignment has expired, and no
    /// other live runner holds the fleet's sandbox, bumping the monotonic token
    /// and recording the sticky hint. Answers `None` when a live runner still
    /// holds it — an ordinary outcome the caller reads as "try the next
    /// candidate", not a failure.
    ///
    /// The assignment PRECEDES the event read by design: a loser has consumed no
    /// event, so nothing is orphaned by losing.
    ///
    /// A test seam: production assigns through the `won_thread` step of the assignment head
    /// (`assign_turn_head.sql`), which runs this same upsert beside the head
    /// read, so this method is not compiled into a build without `test-util`.
    ///
    /// # Errors
    /// Reports a datastore that would not answer. A refused assignment is
    /// `Ok(None)`.
    pub async fn assign(
        &self,
        fleet_id: &Uuid7,
        runner_id: &Uuid7,
        ttl: Duration,
    ) -> Result<Option<WonAssignment>> {
        let owner = RunOwner::Fleet(fleet_id.clone());
        let mut connection = self.pool().acquire().await?;
        let won: Option<Won> = sqlx::query_as(sql::shared::ASSIGN_THREAD)
            .bind(owner.fleet_id())
            .bind(owner.draft_id())
            .bind(runner_id)
            .bind(ttl)
            .bind(self.clock().offline_after())
            .bind(sql::RUNNER_STATUS_ACTIVE)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_ASSIGN))?;
        let Some(won) = won else {
            return Ok(None);
        };
        let outcome = held_assignment(won.held, won.holder.as_ref().map(Uuid7::as_str), runner_id);
        if outcome == Some(HeldAssignment::OtherAfterLapse) {
            producers::fleet::hold::assigned(HeldAssignment::OtherAfterLapse);
        }
        Ok(Some(WonAssignment {
            fence: Fence::from_i64(won.fencing_token),
            held_until: won.run_expires_at,
            resume_hold: outcome == Some(HeldAssignment::Holder),
        }))
    }

    /// Free a thread whose assignment issued no run, so the fleet's next event is
    /// assignable.
    ///
    /// Token-guarded: frees it only while `fence` is still the live token, so a
    /// holder superseded by a reassignment cannot free the CURRENT holder's thread and
    /// hand one fleet to two runners. Idempotent — a no-op when the row is gone
    /// or the token has moved on.
    ///
    /// Called on every post-assignment path that does not issue a run, so an
    /// abandoned assignment costs one poll rather than a full TTL of silence on that
    /// fleet. It also drops the sticky hint the assignment wrote: no run happened on
    /// this runner, and a hint left behind would sort a fleet that keeps
    /// stopping first for it on every poll of the partition.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn release(&self, fleet_id: &Uuid7, fence: Fence) -> Result<()> {
        let mut connection = self.pool().acquire().await?;
        // A fleet's thread is keyed by the fleet's id.
        sqlx::query(sql::shared::RELEASE_RUNLESS_THREAD)
            .bind(fleet_id)
            .bind(fence.as_i64())
            .execute(&mut *connection)
            .await
            .map_err(query(CONTEXT_RELEASE))?;
        Ok(())
    }

    /// Frees an assignment that issued no run, best-effort.
    ///
    /// The one place every post-assignment ending without a run lets go: the
    /// assignment pass's faults, and the pull's refusals, retries, parks and
    /// faults. The caller already has its answer, and a release that fails
    /// changes nothing it can act on: the assignment lapses at its expiry, which is
    /// the cost every such ending paid before this existed. So the failure is
    /// logged and the answer stands.
    pub(crate) async fn let_go(&self, fleet_id: &Uuid7, fence: Fence) {
        if let Err(failure) = self.release(fleet_id, fence).await {
            let Logged { error_code, reason } = failure.logged();
            let fleet_id = fleet_id.as_str();
            {
                let (event, message) = (
                    &EVENT_ASSIGNMENT_RELEASE_FAILED,
                    format_args!(
                        "an assignment that issued no run was not freed; it lapses at its expiry"
                    ),
                );
                tracing::warn!(message, error_code, event, fleet_id, reason);
            };
        }
    }

    #[cfg(any(test, feature = "test-util"))]
    /// The release a finished run owes, on a connection the caller already
    /// holds. Keeps the sticky hint: this runner did run the fleet. Records
    /// `hold_expires_at` too, the hold the run's sandbox is kept under, or clears
    /// it: the fencing guard means a superseded holder records nothing.
    ///
    /// The report path needs it: freeing the thread makes the fleet's next event
    /// assignable, and doing that before the run's result is durable would let a
    /// fresh run race a half-written finalize. Inside the settle's
    /// transaction there is no half-written state to race — the freed thread
    /// becomes visible at the same instant the terminal row does — which is
    /// what lets this be a guarantee rather than the ordering comment it was.
    ///
    /// It takes a connection so it adds no acquire (RULE CNX).
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn release_through(
        &self,
        connection: &mut PgConnection,
        fleet_id: &Uuid7,
        fence: Fence,
        asked_hold: Option<Timestamp>,
    ) -> Result<()> {
        sqlx::query(sql::shared::RELEASE_THREAD)
            .bind(fleet_id)
            .bind(fence.as_i64())
            .bind(asked_hold.map(jiff_sqlx::ToSqlx::to_sqlx))
            .bind(self.clock().sandbox_idle_hold())
            .execute(&mut *connection)
            .await
            .map_err(query(CONTEXT_RELEASE))?;
        Ok(())
    }
}

/// Counts the holder's assignment on its live hold, on the exit that hands out a
/// run resuming it. Not at the assignment: the holder's reassignment resumes nothing,
/// since the event's first attempt may have run in that sandbox.
pub(crate) fn count_resumed(acquired: &Acquired) {
    if acquired.resume_hold {
        producers::fleet::hold::assigned(HeldAssignment::Holder);
    }
}

#[cfg(any(test, feature = "test-util"))]
/// Who won an assignment on a fleet whose thread held a sandbox live at `now`, from
/// the thread's prior `hold_expires_at` and `last_runner_id`, or `None` when it held
/// none. A hold at or past its deadline binds nobody at the assignment's own
/// predicate, so it is no hold here either.
fn held_assignment(held: bool, holder: Option<&str>, runner_id: &Uuid7) -> Option<HeldAssignment> {
    held.then(|| {
        if holder == Some(runner_id.as_str()) {
            HeldAssignment::Holder
        } else {
            HeldAssignment::OtherAfterLapse
        }
    })
}

#[cfg(test)]
#[path = "affinity_tests.rs"]
mod tests;
