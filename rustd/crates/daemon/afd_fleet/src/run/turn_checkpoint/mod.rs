//! Event-owned recovery uses ordinary run authority, independently of native tasks.

mod allocation;
mod commit;
mod files;
mod folds;
mod guard;
mod head;
mod maintenance;
mod manifest;
mod objects;
mod pages;
mod pins;
mod plan;
mod refusal;
mod restore;
mod root;
mod row;
mod signing;
mod store;
mod transaction;
mod transfer;

use af_core::id::Uuid7;

pub use refusal::TurnCheckpointRefusal;
pub use store::TurnCheckpoints;

/// Presented ownership; only the locked guard proves the current connection and fence.
#[derive(Debug, Clone, Copy)]
pub struct TurnRun<'a> {
    /// Authenticated physical runner.
    pub runner_id: &'a Uuid7,
    /// Existing ordinary run; this path never allocates another thread.
    pub run_id: &'a Uuid7,
    /// Current run and thread must both equal this fence.
    pub fencing_token: u64,
}

/// How long a runner's upload link stays valid, by default.
pub(crate) const TRANSFER: std::time::Duration = std::time::Duration::from_mins(15);
/// How long a restoring checkpoint is pinned, by default.
pub(crate) const PIN: std::time::Duration = std::time::Duration::from_mins(2);
/// How long a settled turn's checkpoint is kept, by default.
pub(crate) const RETENTION: std::time::Duration = af_core::timing::DAY.saturating_mul(7);
/// How long an uncommitted upload is kept, by default.
pub(crate) const ORPHAN: std::time::Duration = std::time::Duration::from_hours(1);
const READY_GENERATIONS: i64 = 2;
const WRITER: &str = "writer";
const READER: &str = "reader";

#[cfg(test)]
mod guard_tests;
#[cfg(test)]
mod refusal_tests;
#[cfg(test)]
mod root_tests;
