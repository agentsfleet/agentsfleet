//! Explicit registered-model metadata, scoped to the workspace's billing owner.
//!
//! Each is a `.sql` file beside this module, checked against the rebuilt
//! schema by `afd_db`'s `test_every_sql_file_prepares`. Both project the same
//! columns under the same names, so one row type decodes either.
//!
//! # The resolution version
//!
//! Ciphertext changes even when rotation shares a millisecond with its
//! predecessor. Hashing its sealed bytes in the database detects that change
//! without returning or opening them. JSON array boundaries prevent collisions
//! between a model reference and the metadata fields beside it. The catalogue's
//! `resource_version` rides along, so a repriced model is a new generation.

/// One tenant registry entry, priced, with its credential still a usable key.
///
/// `$1` owner, `$2` opaque reference. Comparing the UUID as text rejects an
/// unknown opaque reference as absence without attempting an unsafe UUID cast.
pub(super) const ENTRY: &str = include_str!("entry.sql");

/// The platform default, priced. The platform choice is explicit; an absent
/// owner entry never selects it.
///
/// `$1` owner, `$2` the reference, which only has to be present.
pub(super) const PLATFORM: &str = include_str!("platform.sql");
