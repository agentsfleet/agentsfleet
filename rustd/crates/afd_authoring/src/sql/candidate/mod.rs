//! Immutable versions and the single draft pointer changed under its row lock.

/// `$1` candidate, `$2` draft.
pub(crate) const READ: &str = include_str!("read.sql");

/// `$1` id, `$2` draft, `$3` revision, `$4` definition, `$5` digest,
/// `$6` policy semantics.
pub(crate) const INSERT: &str = include_str!("insert.sql");

/// Moves the draft's head; the head's revision is read back by join, so the
/// pointer is the only fact written.
///
/// `$1` draft, `$2` candidate.
pub(crate) const SELECT_HEAD: &str = include_str!("select_head.sql");

/// A visible Library revision's definition, by its public Library id.
///
/// `$1` public Library id, `$2` revision, `$3` owner tenant, `$4` the public
/// visibility.
pub(crate) const LIBRARY_SOURCE: &str = include_str!("library_source.sql");

/// One numbered revision of a fleet in the workspace, with the fleet's name and
/// the model review settled on.
///
/// `$1` fleet, `$2` workspace, `$3` revision number.
pub(crate) const FLEET_SOURCE: &str = include_str!("fleet_source.sql");
