//! Where the next reconciliation pass carries on from.
//!
//! # The two things a pass forgets, and what each one costs
//!
//! A pass examines a bounded number of fleets and repairs a bounded number of
//! rows on each. Both bounds are round-trip budgets and both are right. What
//! was missing is any memory of where the last pass stopped, and without it
//! each bound turns from a pace into a ceiling.
//!
//! **Across fleets.** [`crate::sql::SELECT_UNDELIVERED_FLEETS`] orders by
//! `fleet_id` — `DISTINCT ON` requires it — so a deployment where more fleets
//! hold undelivered work than one pass examines reads the same lowest-sorting
//! fleets every single pass. Most of them are healthy, and a healthy fleet
//! still spends a slot. A fleet sorting after them is never examined at all,
//! and accepted work on it is never recovered. Not slowly: never.
//!
//! **Within one fleet.** The pass decides a fleet is worth walking by asking
//! the stream about that fleet's OLDEST undelivered receipt. After a partial
//! repair that question has the wrong answer. Lose thirty-three rows, void the
//! first thirty-two, and the replay sweeper re-appends those thirty-two with
//! new, live receipts. The oldest undelivered row is now one of them, the
//! stream holds it, and the fleet reports healthy while the thirty-third row's
//! receipt is still gone. It stays gone until those thirty-two are delivered
//! and the head moves — which on a fleet whose runner is not consuming is
//! indefinitely.
//!
//! # What this remembers, and what it deliberately does not
//!
//! One fleet id, and a bounded set of fleets whose last walk filled its batch.
//! Nothing is stored per row, nothing is stored per healthy fleet, and nothing
//! is stored in Postgres: the whole structure is two fields on the sweeper.
//!
//! That makes it per-process, and a restart resets it. The cost of the reset is
//! bounded and worth naming: every fleet returns to head-probe examination, so
//! a fleet mid-repair falls back to the shortcut above until its restored rows
//! drain. Recovery gets slower, never unreachable. A durable cursor would fix
//! that and would put a write on the recovery path to buy it; the trade is
//! recorded here rather than made silently.

use std::collections::VecDeque;

/// The fleet id every real fleet sorts above.
///
/// Not a sentinel standing in for "no cursor yet": `core.fleets` constrains its
/// primary key to `UUIDv7` (`ck_fleets_id_uuidv7`, on the fourteenth character),
/// so the nil UUID is not a representable fleet id and `fleet_id > NIL` is a
/// true lower bound on the column's domain. That is what lets the scan keep ONE
/// spelling with a cursor bound always present, instead of a second near-
/// identical statement for the first pass — the kind of pair
/// [`crate::sql`]'s own module note warns about.
pub(crate) const FIRST_FLEET: &str = "00000000-0000-0000-0000-000000000000";

/// One fleet's resume point inside its own undelivered rows.
///
/// The logical event id's two integers, which is the order
/// [`crate::sql::SELECT_UNDELIVERED_ON_FLEET`] reads in and the order its index
/// is built on, so resuming is an index bound rather than a scan and a skip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RowKey {
    /// The admission's creation instant.
    pub(crate) created_at: i64,
    /// Its tiebreak inside that millisecond.
    pub(crate) seq: i64,
}

impl RowKey {
    /// The key every admission sorts above.
    ///
    /// `created_at` is a Unix millisecond and `seq` is an identity column that
    /// starts at one, so neither is ever zero on a row this daemon wrote. Same
    /// argument as [`FIRST_FLEET`], and the same payoff: one statement.
    pub(crate) const FIRST: Self = Self {
        created_at: 0,
        seq: 0,
    };
}

/// A fleet whose walk stopped short of the end of its lost rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Repair {
    /// Which fleet.
    pub(crate) fleet_id: String,
    /// The row its last walk stopped at.
    pub(crate) after: RowKey,
}

/// Where the next pass resumes, across fleets and inside them.
#[derive(Debug, Default)]
pub struct Progress {
    /// The fleet the next head-probe sweep starts after.
    after: Option<String>,
    /// Fleets mid-repair, oldest first, capped at one pass's fleet budget.
    repairing: VecDeque<Repair>,
}

impl Progress {
    /// Where the next head-probe sweep starts.
    pub(crate) fn resume_from(&self) -> &str {
        self.after.as_deref().unwrap_or(FIRST_FLEET)
    }

    /// Records what the head-probe sweep saw and where it should start next.
    ///
    /// A sweep that filled its budget resumes after the last fleet it read. One
    /// that did not has reached the end of the ledger, so the next pass starts
    /// over — which is the whole of the rotation, and the reason a fleet
    /// sorting below the cursor is reachable rather than skipped forever.
    ///
    /// A cursor naming a fleet that has since been deleted needs no handling:
    /// the bound is a strict inequality on a value, not a reference to a row,
    /// so the next scan simply begins at the fleet after where that one was.
    pub(crate) fn swept(&mut self, last_seen: Option<String>, budget_filled: bool) {
        self.after = if budget_filled { last_seen } else { None };
    }

    /// Takes the fleets whose repair the next pass should continue.
    ///
    /// Drained rather than borrowed: a fleet is off the list for the duration of
    /// its walk and goes back on only if the walk says there is more, so a pass
    /// that fails partway cannot leave a fleet queued twice.
    pub(crate) fn resume_repairs(&mut self, budget: i64) -> Vec<Repair> {
        let budget = usize::try_from(budget).unwrap_or(usize::MAX);
        let taken = budget.min(self.repairing.len());
        self.repairing.drain(..taken).collect()
    }

    /// Notes what one fleet's walk did, and whether it has further to go.
    ///
    /// Answers `false` only when there IS more to do and the set had no room
    /// for it — the caller logs that, because a declined repair is the one case
    /// where this structure trades coverage speed for its memory bound. The
    /// fleet is not lost: the head probe still examines it on a later pass, once
    /// the rows this pass restored have been delivered.
    pub(crate) fn walked(
        &mut self,
        fleet_id: &str,
        stopped_at: Option<RowKey>,
        budget: i64,
    ) -> bool {
        let Some(after) = stopped_at else {
            return true;
        };
        let capacity = usize::try_from(budget).unwrap_or(usize::MAX);
        if self.repairing.len() >= capacity {
            return false;
        }
        self.repairing.push_back(Repair {
            fleet_id: fleet_id.to_owned(),
            after,
        });
        true
    }

    /// Whether any fleet is known to have lost rows this pass did not reach.
    ///
    /// The pacing question: a pass that voided nothing but left a repair queued
    /// has found real loss and should come back at the recovering interval, not
    /// the idle one.
    #[must_use]
    pub fn is_resuming(&self) -> bool {
        !self.repairing.is_empty()
    }
}

#[cfg(test)]
mod tests;
