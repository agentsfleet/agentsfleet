//! Runtime model reads. Sealed generations are hashed in Postgres; setup
//! review never receives a key.
//!
//! Each is a `.sql` file beside this module, checked against the rebuilt
//! schema by `afd_db`'s `test_every_sql_file_prepares`. Both project the same
//! columns under the same names, so one row type decodes either.

/// One tenant registry entry, with its credential and the catalogue ceiling.
///
/// The compatible provider may be keyless and may host a model the catalogue
/// does not carry; every named provider needs both.
///
/// `$1` owner · `$2` opaque reference · `$3` the compatible provider's name.
pub(super) const ENTRY: &str = include_str!("entry.sql");

/// The platform default, with its credential.
///
/// `$1` owner · `$2` the reference, which only has to be present · `$3` the
/// compatible provider's name.
pub(super) const PLATFORM: &str = include_str!("platform.sql");
