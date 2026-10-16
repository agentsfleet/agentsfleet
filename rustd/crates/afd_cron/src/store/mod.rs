//! The schedule table's reads and writes, and the fence that serialises them.
//!
//! # Every mutation leaves the row CLAIMED
//!
//! A create, an edit and a `:sync` all end with `sync_status = syncing`, a
//! token, and a lease. That is not bookkeeping: the caller is expected to go and
//! talk to the external scheduler next, and the claim is what stops a second
//! caller doing the same thing at the same time. The pair to every claim is a
//! finalize, which releases the fence and records whether the push worked.
//!
//! A caller that claims and then dies leaves a lease that expires. The next
//! claim takes the row because the fence predicate admits a lease in the past —
//! which is why the lease exists rather than a plain "held" flag, and why a
//! crashed syncer costs a lease window rather than a stuck schedule.
//!
//! # Absent is `Ok(None)`, not an error
//!
//! Reads hide foreign rows with `Ok(None)`. Conditional edits return typed
//! refusals carrying a current tag or state; neither can disclose a foreign row.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::Db;

use crate::error::Result;

mod admit;
mod create;
mod decode;
mod edit;
mod fence;
mod guarded;
mod input;
mod owner;
mod read;
pub(crate) mod refused;
mod target;

pub use self::edit::EditRequest;
pub use self::input::{Change, FireTarget, NewSchedule};
pub use self::refused::Refused;

/// The context a failed read reports under.
const CONTEXT_READ: &str = "read a schedule";

/// The context a failed write reports under.
const CONTEXT_WRITE: &str = "write a schedule";

/// How long a syncer's hold on a row lasts before another may take it.
///
/// Long enough for an upstream round trip and its retries, short enough that a
/// syncer killed mid-push does not strand a schedule for a person watching the
/// list.
pub const SYNC_LEASE_MS: i64 = 30_000;

/// The schedules table, over an already-connected pool.
///
/// Cheap to clone: [`Db`] is a handle over a shared pool.
#[derive(Debug, Clone)]
pub struct Schedules {
    /// Where every statement here runs.
    database: Db,
    /// Where a new schedule's identifier comes from.
    ///
    /// Held rather than taken per call for the reason `afd_admin::Models` holds
    /// one: the source is chosen by the binary, and a suite drives the mocked
    /// one so a created row's identifier is a fact a test can name.
    entropy: Entropy,
}

impl Schedules {
    /// Binds the store to an already-connected pool.
    #[must_use]
    pub const fn new(database: Db, entropy: Entropy) -> Self {
        Self { database, entropy }
    }

    /// A new schedule's identifier, ordered by the instant it was minted.
    ///
    /// `afd_admin::Models::mint`'s shape: the instant leads so rows sort by
    /// creation without a second column, and the entropy is what makes two
    /// minted in the same millisecond distinct.
    pub(crate) fn mint(&self, now: UnixMillis) -> Result<Uuid7> {
        Ok(self.entropy.uuid7(now)?)
    }
}
