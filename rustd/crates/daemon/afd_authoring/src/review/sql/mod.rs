//! Review reads observe current access; none of them writes.

/// The fleet's current setup revision; NULL before its first revision lands.
///
/// `$1` fleet, `$2` workspace.
pub(super) const TARGET: &str = include_str!("target.sql");

/// The creator's account membership as a version digest; no row when the
/// creator no longer belongs to the workspace owner's account. Locks only the
/// person's row, as `crate::sql::access::PERSON` explains.
///
/// `$1` creator, `$2` workspace.
pub(super) const AUTHORITY: &str = include_str!("authority.sql");

/// The owner's runtime default: its selected tenant model, or the platform
/// reference when it selected none.
///
/// `$1` tenant, `$2` the platform reference.
pub(super) const DEFAULT_MODEL: &str = include_str!("default_model.sql");

/// `$1` fleet, `$2` service.
pub(super) const GRANT: &str = include_str!("grant.sql");

/// The routing installs a reviewed handle names, as a version digest, and
/// whether every one grants the scopes the service needs.
///
/// `$1` workspace, `$2` provider, `$3` external account, `$4` required scopes.
pub(super) const CONNECTION: &str = include_str!("connection.sql");
