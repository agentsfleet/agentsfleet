//! The runaway brake: how often one action has repeated inside its window.
//!
//! # It runs before the approval gate, and it fails the other way
//!
//! The approval gate fails CLOSED — a Redis outage leaves an event waiting
//! rather than releasing it. This check fails OPEN. That asymmetry is not an
//! inconsistency: the gate answers "did a human say yes", where the safe answer
//! under uncertainty is to keep waiting, while this answers "has this repeated
//! suspiciously often", where the safe answer under uncertainty is that we do
//! not know and must not pause a healthy fleet on a guess.
//!
//! # It is only ever reached on a FIRST encounter
//!
//! The counter is an increment. Re-polling a parked event through it would
//! count one waiting human as N runaway attempts, and eventually auto-kill the
//! fleet for being patient. The caller's ordering — a recorded gate is read and
//! honoured before any policy is consulted — is what keeps that from happening,
//! and it is why this module takes no `RefState`: by the time it is called,
//! that question is settled.

use crate::gate::store::{Gates, key};
use afd_core::id::Uuid7;
use afd_fleet_runtime::config::AnomalyRule;

/// A repetition crossed its threshold.
const EVENT_AUTO_KILL: &str = "gate_anomaly_auto_kill";

/// The counter could not be read.
const EVENT_COUNTER_UNAVAILABLE: &str = "gate_anomaly_counter_unavailable";

/// What the counters make of one action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anomaly {
    /// Nothing has crossed a threshold.
    Normal,
    /// A repetition tripped a rule; the fleet is paused.
    AutoKill,
}

impl Gates {
    /// Count this action against every rule, and say whether one tripped.
    ///
    /// Each rule owns its own window, so each gets its own increment — a single
    /// shared counter could not answer "ten in a minute" and "a hundred in an
    /// hour" at once, which is the whole reason a policy may declare several.
    ///
    /// Answers a verdict rather than a `Result`, which is the one place in
    /// this crate that is right: a counter that cannot be read is absorbed
    /// into [`Anomaly::Normal`] here, and there is no caller decision left to
    /// make. Handing back an `Err` would offer a choice whose only correct
    /// answer this module already knows.
    pub async fn anomaly(
        &self,
        fleet_id: &Uuid7,
        tool: &str,
        action: &str,
        rules: &[AnomalyRule],
    ) -> Anomaly {
        let key = key::anomaly(fleet_id.as_str(), tool, action);
        for rule in rules {
            let Some(count) = self.count(&key, rule.window_s.get()).await else {
                return Anomaly::Normal;
            };
            if count >= i64::from(rule.repeats.get()) {
                let fleet = fleet_id.as_str();
                let threshold = rule.repeats.get();
                tracing::warn!(
                    event = EVENT_AUTO_KILL,
                    fleet_id = fleet,
                    tool,
                    action,
                    count,
                    threshold,
                    "one action repeated past its threshold; the fleet is stopped"
                );
                return Anomaly::AutoKill;
            }
        }
        Anomaly::Normal
    }

    /// This action's count inside `window_s`, or `None` if it cannot be read.
    ///
    /// The fail-open decision, taken HERE and once, rather than at each of the
    /// three `catch return true` sites the Zig's balance gate spreads its own
    /// version across.
    async fn count(&self, key: &str, window_s: u32) -> Option<i64> {
        match self.queue().increment_in_window(key, window_s).await {
            Ok(count) => Some(count),
            Err(fault) => {
                let reason = fault.to_string();
                tracing::warn!(
                    event = EVENT_COUNTER_UNAVAILABLE,
                    reason,
                    "the anomaly counter could not be read; the action is admitted \
                     rather than pausing a fleet on a guess"
                );
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Anomaly;

    #[test]
    fn the_two_outcomes_are_distinct() {
        // A trivial assertion over a two-arm enum, and it is here for one
        // reason: the behaviour that matters — a threshold crossing, a Redis
        // outage admitting — is exercised by the integration suite against a
        // live counter, and a unit test that mocked the counter would prove
        // only that the mock returns what it was told.
        assert_ne!(Anomaly::Normal, Anomaly::AutoKill);
    }
}
