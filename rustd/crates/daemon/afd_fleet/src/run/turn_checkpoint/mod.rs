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

/// How long a checkpoint's uploads or restore may run in total, by default.
pub(crate) const TRANSFER: std::time::Duration = std::time::Duration::from_mins(15);
/// How long a restoring checkpoint is pinned, by default.
pub(crate) const PIN: std::time::Duration = std::time::Duration::from_mins(2);
/// How long a settled turn's checkpoint is kept, by default.
pub(crate) const RETENTION: std::time::Duration = af_core::timing::DAY.saturating_mul(7);
/// How long an uncommitted upload is kept, by default.
pub(crate) const ORPHAN: std::time::Duration = std::time::Duration::from_hours(1);
const READY_GENERATIONS: i64 = 2;

/// The `thread.replay_receipts` scope of a turn's checkpoint allocations: one
/// retry key per operation for the turn's life, shared by its replacement runs.
pub(crate) fn receipt_scope(workspace: &Uuid7, turn: &Uuid7) -> String {
    format!("{workspace}:turn:{turn}:checkpoint")
}

/// Keeps one `thread.replay_receipts` key: `$1` scope, `$2` operation, `$3`
/// request digest, `$4` what the first request made. A durable task's
/// adoption keeps its retry key through the same statement.
pub(crate) const RECEIPT_INSERT: &str = include_str!("sql/operation_insert.sql");

/// Reads one `thread.replay_receipts` key into a [`Receipt`]: `$1` scope,
/// `$2` operation.
pub(crate) const RECEIPT_REPLAY: &str = include_str!("sql/operation_replay.sql");

/// A retry key's answer: what the first request made, and that request's
/// digest, which a retry must match.
#[derive(sqlx::FromRow)]
pub(crate) struct Receipt {
    /// What the first request made.
    pub(crate) resource_id: Uuid7,
    /// The first request's digest.
    pub(crate) request_digest: String,
}
const WRITER: &str = "writer";
const READER: &str = "reader";

#[cfg(test)]
mod files_tests;
#[cfg(test)]
mod guard_tests;
#[cfg(test)]
mod objects_tests;
#[cfg(test)]
mod pages_tests;
#[cfg(test)]
mod refusal_tests;
#[cfg(test)]
mod root_tests;
#[cfg(all(test, feature = "test-util"))]
mod store_tests;
