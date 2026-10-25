//! Semantic-memory persistence cadence is independent of model-window checkpoints.

use afd_wire::policy::ContextBudget;

/// When a run writes its memory back mid-run: every `every` calls, never when
/// `every` is zero.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Checkpoints {
    every: u32,
    since: u32,
}

impl Checkpoints {
    /// The cadence a lease's policy sets.
    pub(crate) const fn new(budget: &ContextBudget<'_>) -> Self {
        Self {
            every: budget.memory_checkpoint_every,
            since: 0,
        }
    }

    /// A cadence that is never due: a child loop's, whose stores the push
    /// before the report carries.
    pub(crate) const fn never() -> Self {
        Self { every: 0, since: 0 }
    }

    /// Counts one finished call; `true` when it completes a cadence.
    pub(crate) fn due(&mut self) -> bool {
        if self.every == 0 {
            return false;
        }
        self.since += 1;
        let due = self.since >= self.every;
        if due {
            self.since = 0;
        }
        due
    }
}

#[cfg(test)]
mod tests;
