//! Event-owned recovery uses ordinary lease authority, independently of native tasks.

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
pub struct TurnLease<'a> {
    /// Authenticated physical runner.
    pub runner_id: &'a Uuid7,
    /// Existing ordinary lease; this path never allocates another slot.
    pub lease_id: &'a Uuid7,
    /// Current lease and slot must both equal this fence.
    pub fencing_token: u64,
}

const TRANSFER_SECONDS: i32 = 15 * 60;
const PIN_SECONDS: i32 = 2 * 60;
const READY_GENERATIONS: i64 = 2;
const WRITER: &str = "writer";
const READER: &str = "reader";

#[cfg(test)]
mod guard_tests;
#[cfg(test)]
mod refusal_tests;
#[cfg(test)]
mod root_tests;
