//! Proving a runner holds a live run on a fleet, and what token it holds.
//!
//! The memory verbs authorize differently from every other verb in this plane.
//! A report names its RUN and the statement finds the fleet; a memory call
//! names its FLEET — the runner already holds it in the run payload, so
//! naming it beats inferring it from ambient state — and the statement has to
//! find the run.
//!
//! # The token is `u64`, and the column is not
//!
//! `fencing_token` is server-issued and monotonic, so no value this daemon writes
//! is negative. One edited out of band could be, and [`u64::try_from`] refuses
//! it: its `Err` becomes the `sequence_corrupt` refusal rather than a token
//! that wrapped to a huge positive value.

use af_core::id::Uuid7;
use sqlx::FromRow as _;

use crate::error::{Result, query, sequence_corrupt};
use crate::run::sql;
use crate::run::standing::{fence_current, fence_holds};
use crate::run::store::Runs;

/// Statement name, for the context a query failure carries.
const CONTEXT_FENCE: &str = "live fence lookup";

/// The run's own token and the fleet's live fencing sequence, if this runner
/// holds a live run on it.
///
/// The thread row's token is the live sequence, so a reassignment that bumped it
/// strands the old holder BELOW it. A fleet with no thread row (only a fleet's
/// deletion removes one) has no live sequence, and so no live fence.
///
/// Where one runner briefly holds two live runs on the fleet — a superseded
/// one and its successor — the highest token wins: tokens rise with every
/// issue, where `created_at` is one daemon replica's clock.
///
/// `$1` runner, `$2` fleet, `$3` the active status; the deadline is read on
/// `statement_timestamp()`.
const SELECT_LIVE_FENCE_BY_FLEET: &str = include_str!("select_live_fence_by_fleet.sql");

/// The same fence, addressed by run id when the caller already holds one.
///
/// Keyed by run AND fleet, so a run that exists but belongs to another
/// fleet yields no row — the IDOR cross-check IS the `WHERE`, not a comparison
/// the handler has to remember to make afterwards.
///
/// `$1` run, `$2` runner, `$3` fleet, `$4` the active status; the deadline is
/// read on `statement_timestamp()`.
const SELECT_LIVE_FENCE_BY_RUN: &str = include_str!("select_live_fence_by_run.sql");

/// A live run's own token beside its fleet's live sequence.
///
/// Both, because neither alone decides. A run row stays `active` after a
/// reassignment has moved the fleet past it, until the reassignment's issue expires it,
/// so "the run is live" says nothing about whether it still holds the fleet;
/// and a token compared only against the sequence admits any value above it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Fence {
    /// The token the run was issued with.
    own: i64,
    /// The fleet's live sequence.
    live_seq: i64,
}

impl Fence {
    /// The two columns a fence read answers, refused when either cannot be a
    /// sequence: a negative live sequence has no safe reading (see
    /// [`sequence_corrupt`]), and a negative token is no run's.
    fn new(own: i64, live_seq: i64) -> Result<Self> {
        if own < 0 || live_seq < 0 {
            return Err(sequence_corrupt());
        }
        Ok(Self { own, live_seq })
    }

    /// Whether no reassignment has moved the fleet past this run: the first half
    /// of the rule [`Self::holds`] applies, from the same function.
    pub(crate) const fn current(self) -> bool {
        fence_current(self.own, self.live_seq)
    }

    /// Whether this run still holds the fleet and `presented` is its own
    /// token: the rule report and renew apply, through the same function.
    pub(crate) fn holds(self, presented: u64) -> bool {
        fence_holds(self.own, self.live_seq, presented)
    }

    /// The run's own token, for the line a refusal logs.
    pub(crate) const fn own(self) -> i64 {
        self.own
    }

    /// The fleet's live sequence, for the line a refusal logs.
    pub(crate) const fn live_seq(self) -> i64 {
        self.live_seq
    }
}

impl Runs {
    /// The run's fence, if `runner_id` holds a live run on the fleet.
    ///
    /// `None` means no live run — expired, reassigned, or never held. A run
    /// that is live but superseded answers a [`Fence`] that is not
    /// [`current`](Fence::current); a load refuses both, because a runner
    /// may read a fleet's memory only while it is actually running it.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a stored sequence that is
    /// not a sequence.
    pub(crate) async fn live_fence_for_fleet(
        &self,
        runner_id: &Uuid7,
        fleet_id: &Uuid7,
    ) -> Result<Option<Fence>> {
        let found = sqlx::query(SELECT_LIVE_FENCE_BY_FLEET)
            .bind(runner_id)
            .bind(fleet_id)
            .bind(sql::RUN_STATUS_ACTIVE);
        self.read_fence(found).await
    }

    /// The same fence, for a caller naming the run it believes it holds.
    ///
    /// Keyed by run AND fleet, so a run belonging to another fleet answers
    /// `None` — the cross-check that stops a runner reaching one fleet's memory
    /// with another fleet's run is the statement's `WHERE`, not a comparison
    /// this code has to remember to make.
    ///
    /// # Errors
    /// As [`Runs::live_fence_for_fleet`].
    pub(crate) async fn live_fence_for_run(
        &self,
        runner_id: &Uuid7,
        run_id: &Uuid7,
        fleet_id: &Uuid7,
    ) -> Result<Option<Fence>> {
        let found = sqlx::query(SELECT_LIVE_FENCE_BY_RUN)
            .bind(run_id)
            .bind(runner_id)
            .bind(fleet_id)
            .bind(sql::RUN_STATUS_ACTIVE);
        self.read_fence(found).await
    }

    /// Run a prepared fence statement and check its columns are sequences.
    async fn read_fence(&self, statement: afd_db::statement::Bound<'_>) -> Result<Option<Fence>> {
        let mut connection = self.pool().acquire().await?;
        let Some(row) = statement
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_FENCE))?
        else {
            return Ok(None);
        };
        let read = LiveFence::from_row(&row).map_err(query(CONTEXT_FENCE))?;
        Fence::new(read.own, read.live_seq).map(Some)
    }
}

/// A fence statement's answer: the run's own token and the fleet's live one.
#[derive(sqlx::FromRow)]
struct LiveFence {
    own: i64,
    live_seq: i64,
}

#[cfg(test)]
mod tests;
