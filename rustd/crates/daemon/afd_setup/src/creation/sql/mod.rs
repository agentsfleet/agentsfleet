//! A fleet's create, read, change and delete, always workspace-scoped.

/// The fleet row alone, locked before [`LOCKED`] reads it for a write.
///
/// `$1` fleet, `$2` workspace.
pub(super) const LOCK: &str = include_str!("lock.sql");

/// The fleet with its counters and open gates. After [`LOCK`], read under a
/// fresh statement snapshot that already includes whichever writer held the
/// lock before; alone, for a read that holds nothing.
///
/// `$1` fleet, `$2` workspace.
pub(super) const LOCKED: &str = include_str!("locked.sql");

/// Whether the workspace holds the fleet, locking nothing.
///
/// `$1` fleet, `$2` workspace.
pub(super) const EXISTS: &str = include_str!("exists.sql");

/// The fleet row and its thread; no row when the workspace already holds the
/// name.
///
/// `$1` id, `$2` workspace, `$3` tenant, `$4` name, `$5` status, `$6` model,
/// `$7` HTTP egress, `$8` package domains, `$9` resource version.
pub(super) const INSERT: &str = include_str!("insert.sql");

/// `$1` fleet, `$2` workspace, `$3` status, `$4` name, `$5` model, `$6` HTTP
/// egress, `$7` package domains.
pub(super) const UPDATE: &str = include_str!("update.sql");

/// Marks every active fleet run before an archive or a delete commits.
///
/// `$1` fleet, `$2` the active run status.
pub(super) const CANCEL_RUNS: &str = include_str!("mark_runs_cancel_requested.sql");

/// The owner's runtime default: its selected tenant model, or the platform
/// reference when it selected none.
///
/// `$1` tenant, `$2` the platform reference.
pub(super) const DEFAULT_MODEL: &str = include_str!("default_model.sql");

/// The turn a create admitted, by its producer key.
///
/// `$1` fleet, `$2` producer, `$3` producer key.
pub(super) const FIRST_TURN: &str = include_str!("first_turn.sql");
