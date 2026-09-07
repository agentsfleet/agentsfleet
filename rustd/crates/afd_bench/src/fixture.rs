//! Fixture tenancy: the run prefix every created object carries, and the
//! ledger that proves the sweep removed all of them.
//!
//! # Why a prefix and not a timestamp filter
//!
//! A deployed run shares its environment with whatever else is there, so the
//! sweep has to be able to say "these and only these are mine". A prefix
//! written into every created name answers that exactly, survives a crash, and
//! lets a LATER run clean up an earlier one's orphans — which a filter over
//! "objects created during this run" cannot do, because the process that knew
//! the window is the one that died.
//!
//! The acceptance suite reaches the same conclusion in
//! `ui/packages/app/tests/e2e/acceptance/fixtures/teardown.ts`, which sweeps by
//! run prefix inside a fixture workspace rather than by age.
//!
//! Collecting an EARLIER run's orphans is not built yet: it needs an age floor
//! below which another run's prefix is fair game, and that number is deferred
//! with the deployed-environment follow-up in the spec's Discovery.

use core::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

/// Leading token on every object a lane creates, so a sweep can recognise its
/// own work and a human can recognise it in a console.
pub const PREFIX_TOKEN: &str = "bench";

/// Identifies one run's objects, for the whole life of those objects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunPrefix {
    value: String,
}

impl RunPrefix {
    /// Mint a prefix for a run starting now.
    ///
    /// Wall clock plus process id: two runs on one machine differ by pid, and
    /// two on different machines differ by the millisecond they started. There
    /// is no coordination point to ask for a ticket, and a collision would only
    /// mean one run sweeping another's objects in the same fixture workspace —
    /// which is the outcome the sweep is for.
    #[must_use]
    pub fn mint() -> Self {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        Self {
            value: format!("{PREFIX_TOKEN}-{millis}-{}", std::process::id()),
        }
    }

    /// The prefix itself, for writing into a created object's name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.value
    }

    /// Name an object so the sweep will find it.
    #[must_use]
    pub fn name(&self, suffix: &str) -> String {
        format!("{}-{suffix}", self.value)
    }

    /// Whether a name belongs to this run.
    #[must_use]
    pub fn owns(&self, name: &str) -> bool {
        name.starts_with(&self.value)
    }
}

impl fmt::Display for RunPrefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.value)
    }
}

/// Counts what a run created against what its sweep removed.
///
/// The two numbers go into the result file, and a deployed run is only
/// acceptable when they match — which is a fact the file states rather than a
/// promise the code makes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FixtureLedger {
    created: u64,
    swept: u64,
}

impl FixtureLedger {
    /// An empty ledger, before a run creates anything.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            created: 0,
            swept: 0,
        }
    }

    /// Record objects this run created.
    pub const fn created(&mut self, count: u64) {
        self.created = self.created.saturating_add(count);
    }

    /// Record objects the sweep removed.
    ///
    /// Counts orphans from an earlier run too, which is why [`is_balanced`]
    /// compares with `>=` rather than `==`.
    ///
    /// [`is_balanced`]: Self::is_balanced
    pub const fn swept(&mut self, count: u64) {
        self.swept = self.swept.saturating_add(count);
    }

    /// How many objects this run created.
    #[must_use]
    pub const fn created_count(&self) -> u64 {
        self.created
    }

    /// How many objects the sweep removed.
    #[must_use]
    pub const fn swept_count(&self) -> u64 {
        self.swept
    }

    /// Whether the sweep accounted for everything this run created.
    #[must_use]
    pub const fn is_balanced(&self) -> bool {
        self.swept >= self.created
    }
}

#[cfg(test)]
mod tests;
