//! One run's memory, behind the backend `agentsfleetd` binds the fleet to.
//!
//! [`MemoryBackend`] is what the four memory tools read and write
//! (`docs/architecture/runner_fleet.md` §"Memory backends"). [`Hydrated`] is
//! the default, Postgres through `agentsfleetd`: the window hydrated at lease
//! start, with the run's stores held here and pushed, fenced, before the
//! report. A vendor backend talks to its own service with a key `agentsfleetd`
//! minted for this fleet's namespace, and writes as it goes.

pub mod error;

mod hydrated;

use std::fmt;

use afd_wire::memory::MemoryDelta;

pub use self::error::{Error, Result};
pub use self::hydrated::Hydrated;

/// What a forget did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Forgotten {
    /// Nothing was remembered under the key.
    Unknown,
    /// The entry is gone for the rest of this run; its durable copy stays
    /// until a store under the same key replaces it.
    ForThisRun,
}

/// Where a run's memory lives.
///
/// Bound to one fleet's namespace before the run starts, so every call answers
/// for that fleet alone. A backend that crosses a network boundary logs a
/// `_started` and a `_completed` or `_failed` pair for each call, at `debug`
/// because a run makes many (`docs/LOGGING_STANDARD.md` §4 rules 1 and 3).
#[async_trait::async_trait]
pub trait MemoryBackend: Send + Sync + fmt::Debug {
    /// Stores `entry`, replacing what its key held.
    ///
    /// # Errors
    /// The entry breaks a bound `afd_wire` declares on a stored entry, or the
    /// backend cannot take it.
    async fn store(&mut self, entry: MemoryDelta<'static>) -> Result<()>;

    /// The entries whose key holds `query`, ignoring ASCII case, newest first,
    /// at most `limit`; an empty query holds in every key.
    ///
    /// The model reads what comes back and decides what is relevant: a
    /// substring of the key is the ceiling on search
    /// (`docs/architecture/direction.md`).
    ///
    /// # Errors
    /// The backend cannot answer.
    async fn recall<'m>(&'m self, query: &str, limit: usize) -> Result<Vec<MemoryDelta<'m>>>;

    /// Every entry, or every entry in `category`, newest first.
    ///
    /// # Errors
    /// The backend cannot answer.
    async fn list<'m>(&'m self, category: Option<&str>) -> Result<Vec<MemoryDelta<'m>>>;

    /// Forgets `key`.
    ///
    /// # Errors
    /// The backend cannot answer.
    async fn forget(&mut self, key: &str) -> Result<Forgotten>;

    /// The entries the supervisor still has to push, fenced, before the
    /// report; none for a backend that wrote them as it went.
    fn into_pending(self: Box<Self>) -> Vec<MemoryDelta<'static>>;
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
