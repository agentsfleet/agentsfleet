//! Content-free replay identity is retained independently from private state.
//!
//! A receipt has no foreign key: it outlives the resource it names, so a retry
//! after deletion finds the receipt and a missing resource, which the read
//! reports as `live = false` and the caller answers as Gone
//! (`schema/1980_authoring_replay_receipts.sql`). Every statement here reads
//! the window against `statement_timestamp()`, so a key the read calls expired
//! is one the delete and the sweep call expired too.

/// `$1` scope, `$2` operation, `$3` the replay window, `$4` the resource kind
/// (a [`super::Resource`] spelling) whose table `live` checks.
pub(super) const READ: &str = include_str!("read.sql");

/// Deletes the key's receipt once it is outside the window, so the unique
/// `(scope, operation_id)` key admits the new claim.
///
/// `$1` scope, `$2` operation, `$3` the replay window.
pub(super) const EXPIRE: &str = include_str!("expire.sql");

/// `$1` id, `$2` workspace, `$3` person, `$4` scope, `$5` operation,
/// `$6` request digest, `$7` resource.
pub(super) const INSERT: &str = include_str!("insert.sql");

/// Deletes a bounded batch of receipts outside the window.
///
/// No row lock: `api_runtime` holds no UPDATE on this immutable table, so
/// Postgres would refuse one, and none is needed. A sweep that meets a row a
/// claim is deleting ([`EXPIRE`]) waits for that claim and then finds the row
/// gone; deleting a receipt twice harms nothing.
///
/// `$1` the replay window, `$2` limit.
pub(super) const SWEEP: &str = include_str!("sweep.sql");
