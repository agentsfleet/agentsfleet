//! One run's memory, read and written through `agentsfleetd`.
//!
//! [`Loaded`] is what the four memory tools read and write
//! (`docs/architecture/runner_fleet.md` §"Memory backends and scope"),
//! through `guffbolt`'s memory store: the [`Seed`] `agentsfleetd` loaded
//! at run start — the fleet's own window, the workspace's shared entries for
//! a fleet granted to read them, and whether it may publish — with the run's
//! stores held here and pushed, fenced, before the report. A recall the window
//! cannot fill asks `agentsfleetd` through [`Recall`], a capped number of times
//! per run. Which store holds the memory behind `agentsfleetd` is its choice;
//! the runner holds no credential for any of them.

#![deny(clippy::self_named_module_files)]

pub mod error;

mod loaded;
mod recovery;
mod seed;

pub use self::error::{Error, Result};
pub use self::loaded::Loaded;
pub use self::recovery::MemoryRecovery;
pub use self::seed::{RECALL_MISS_CAP, Recall, Recalled, Seed};

/// What a forget did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Forgotten {
    /// Nothing of this fleet's was remembered under the key.
    Unknown,
    /// The entry is gone for the rest of this run; its durable copy stays
    /// until a store under the same key replaces it.
    ForThisRun,
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

#[cfg(test)]
mod recovery_tests;

#[cfg(test)]
#[path = "shared_tests.rs"]
mod shared_tests;
