//! Content-free replay identity is retained independently from private state.
//!
//! A receipt has no foreign key and no purge mark: it outlives the resource it
//! names, so a retry after deletion finds the receipt and a missing resource,
//! which its caller answers as Gone (`schema/1980_authoring_replay_receipts.sql`).

/// `$1` scope, `$2` operation.
pub(super) const READ: &str = include_str!("read.sql");

/// `$1` id, `$2` workspace, `$3` person, `$4` scope, `$5` operation,
/// `$6` request digest, `$7` resource.
pub(super) const INSERT: &str = include_str!("insert.sql");
