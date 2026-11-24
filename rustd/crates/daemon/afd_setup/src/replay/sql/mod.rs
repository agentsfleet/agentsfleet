//! Content-free replay identity is retained independently from private state.
//!
//! A receipt has no foreign key: it outlives the resource it names, so a retry
//! after deletion finds the receipt and a missing resource, which the read
//! reports as `live = false` and the caller answers as Gone
//! (`afd_db/schema/20261122090800_thread_replay_receipts.sql`). Every statement here reads
//! the window against `statement_timestamp()`, so a key the read calls expired
//! is one the delete calls expired too.

/// `$1` scope, `$2` operation, `$3` the replay window.
pub(super) const READ: &str = include_str!("read.sql");

/// Deletes the key's receipt once it is outside the window, so the unique
/// `(scope, operation_id)` key admits the new claim.
///
/// `$1` scope, `$2` operation, `$3` the replay window.
pub(super) const EXPIRE: &str = include_str!("expire.sql");

/// `$1` scope, `$2` operation, `$3` request digest bound to its actor
/// ([`super::bound`]), `$4` resource.
pub(super) const INSERT: &str = include_str!("insert.sql");
