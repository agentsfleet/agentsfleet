//! The runaway brake: how often one action has repeated inside its window.
//!
//! # It runs before the approval gate, and it fails the other way
//!
//! The approval gate fails CLOSED — an unreadable gate leaves an event waiting
//! rather than releasing it. This check fails OPEN. That asymmetry is not an
//! inconsistency: the gate answers "did a human say yes", where the safe answer
//! under uncertainty is to keep waiting, while this answers "has this repeated
//! suspiciously often", where the safe answer under uncertainty is that we do
//! not know and must not pause a healthy fleet on a guess.
//!
//! # It counts per replica, in memory
//!
//! Each replica keeps its own windows, one per fleet, tool, action and rule
//! window, bounded at [`ANOMALY_KEYS_MAX`] with the oldest window evicted. A
//! runaway spread over R replicas can repeat up to R times a threshold before
//! a pause, and a restart forgets every count. Both are accepted costs: a
//! shared count would put a network round trip on every first encounter, and
//! the brake is a backstop that fails open by design.
//!
//! # It is only ever reached on a FIRST encounter
//!
//! The counter is an increment. Re-polling a parked event through it would
//! count one waiting human as N runaway attempts, and eventually auto-kill the
//! fleet for being patient. The caller's ordering — a recorded gate is read and
//! honoured before any policy is consulted — is what keeps that from happening.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use af_core::id::Uuid7;
use af_fleet_runtime::config::AnomalyRule;

use crate::gate::store::Gates;

/// Windows one replica holds before the oldest is evicted.
pub(crate) const ANOMALY_KEYS_MAX: usize = 10_000;

/// A repetition crossed its threshold.
const EVENT_AUTO_KILL: &str = "gate_anomaly_auto_kill";

/// What the counters make of one action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anomaly {
    /// Nothing has crossed a threshold.
    Normal,
    /// A repetition tripped a rule; the fleet is paused.
    AutoKill,
}

/// One rule's window for one action. The rule's position, not its window,
/// tells two rules apart, so two rules over the same window each keep their
/// own count.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key {
    fleet: Uuid7,
    tool: String,
    action: String,
    rule: usize,
    window_s: u32,
}

#[derive(Debug, Clone, Copy)]
struct Window {
    started: Instant,
    count: u64,
}

/// Every window this replica counts, bounded.
#[derive(Debug)]
pub(crate) struct Brake {
    max: usize,
    windows: Mutex<HashMap<Key, Window>>,
}

impl Brake {
    pub(crate) fn new(max: usize) -> Self {
        Self {
            max,
            windows: Mutex::new(HashMap::new()),
        }
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<Key, Window>> {
        // A count lost to a panic is a count the brake fails open on anyway.
        self.windows.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Counts one call against `key`'s window and answers the new count. A
    /// window starts at its first call and lasts `window_s`.
    fn hit(&self, key: Key, now: Instant) -> u64 {
        let window = Duration::from_secs(u64::from(key.window_s));
        let mut windows = self.lock();
        if let Some(open) = windows.get_mut(&key)
            && now.saturating_duration_since(open.started) < window
        {
            open.count += 1;
            return open.count;
        }
        if !windows.contains_key(&key) && windows.len() >= self.max {
            evict_oldest(&mut windows);
        }
        windows.insert(
            key,
            Window {
                started: now,
                count: 1,
            },
        );
        1
    }

    /// Windows currently held.
    #[cfg(test)]
    fn len(&self) -> usize {
        self.lock().len()
    }
}

fn evict_oldest(windows: &mut HashMap<Key, Window>) {
    let oldest = windows
        .iter()
        .min_by_key(|(_, window)| window.started)
        .map(|(key, _)| key.clone());
    if let Some(oldest) = oldest {
        windows.remove(&oldest);
    }
}

/// Counts one call against every rule's window and answers whether any rule
/// tripped. Every rule counts every call, so a later rule's window is never
/// short of the calls an earlier rule's trip would have hidden.
fn judge(
    brake: &Brake,
    fleet_id: &Uuid7,
    tool: &str,
    action: &str,
    rules: &[AnomalyRule],
    now: Instant,
) -> Option<(u64, u32)> {
    let mut tripped = None;
    for (position, rule) in rules.iter().enumerate() {
        let key = Key {
            fleet: fleet_id.clone(),
            tool: tool.to_owned(),
            action: action.to_owned(),
            rule: position,
            window_s: rule.window_s.get(),
        };
        let count = brake.hit(key, now);
        let threshold = rule.repeats.get();
        if tripped.is_none() && count >= u64::from(threshold) {
            tripped = Some((count, threshold));
        }
    }
    tripped
}

impl Gates {
    /// Count this action against every rule, and say whether one tripped.
    ///
    /// Each rule owns its own window, so each gets its own count — a single
    /// shared counter could not answer "ten in a minute" and "a hundred in an
    /// hour" at once, which is the whole reason a policy may declare several.
    #[must_use]
    pub fn anomaly(
        &self,
        fleet_id: &Uuid7,
        tool: &str,
        action: &str,
        rules: &[AnomalyRule],
    ) -> Anomaly {
        let Some((count, threshold)) =
            judge(self.brake(), fleet_id, tool, action, rules, Instant::now())
        else {
            return Anomaly::Normal;
        };
        let fleet = fleet_id.as_str();
        tracing::warn!(
            event = EVENT_AUTO_KILL,
            fleet_id = fleet,
            tool,
            action,
            count,
            threshold,
            "one action repeated past its threshold; the fleet is stopped"
        );
        Anomaly::AutoKill
    }
}

#[cfg(test)]
#[path = "anomaly_tests.rs"]
mod tests;
