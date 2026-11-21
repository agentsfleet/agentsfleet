//! Private draft rows are always scoped by workspace and creator together.

/// The creator's live draft, locked, with its head revision and source
/// reference resolved by join.
///
/// The expiry check reads `clock_timestamp()` after the lock: a draft that
/// expired while this request waited on it reads as missing. A source whose
/// revision was deleted reads as NULL columns.
///
/// `$1` draft, `$2` workspace, `$3` creator.
pub(crate) const LOCK: &str = include_str!("lock.sql");

/// `$1` id, `$2` workspace, `$3` tenant, `$4` creator, `$5` title, `$6` status,
/// `$7` tenant model (NULL for the platform default), `$8` source type,
/// `$9` source fleet revision, `$10` instance name, `$11` linked fleet, `$12`
/// the initial counter.
pub(crate) const CREATE: &str = include_str!("create.sql");

/// `$1` draft, `$2` and `$3` the in-flight message statuses.
pub(crate) const ACTIVE_MESSAGES: &str = include_str!("active_messages.sql");

/// `$1` draft, `$2` title, `$3` tenant model, `$4` instance name (NULL keeps it).
pub(crate) const PATCH: &str = include_str!("patch.sql");

/// `$1` draft.
pub(crate) const DELETE: &str = include_str!("delete.sql");

/// One page of the creator's live drafts, newest change first.
///
/// `$1` workspace, `$2` creator, `$3` status filter, `$4` fleet filter,
/// `$5` cursor instant, `$6` cursor id, `$7` limit.
pub(crate) const PAGE: &str = include_str!("page.sql");
