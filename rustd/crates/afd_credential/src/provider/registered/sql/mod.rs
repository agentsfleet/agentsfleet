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
//!
//! # Lock order
//!
//! Both reads share-lock the catalogue version, the credential, the model and
//! the entry or default they select. Postgres takes one statement's row locks
//! in whatever order its plan reaches them. The writers take them in a fixed
//! order instead: a catalogue edit takes the version and then the model, and an
//! entry removal or a credential delete takes the credential and then the entry.
//! A read holding the model while it waits on the version, or the entry while it
//! waits on the credential, deadlocks those writers. So the read takes
//! [`LOCK_CATALOGUE`] and then its credential lock first, each as a statement of
//! its own, and the read proper finds both already held.

/// One tenant registry entry, priced, with its credential still a usable key,
/// the credential's endpoint restriction and the model's saved inference
/// settings. Whether those settings route is decided in Rust, not here.
///
/// `$1` owner, `$2` opaque reference. Comparing the UUID as text rejects an
/// unknown opaque reference as absence without attempting an unsafe UUID cast.
pub(super) const ENTRY: &str = include_str!("entry.sql");

/// The platform default, priced. The platform choice is explicit; an absent
/// owner entry never selects it.
///
/// `$1` owner, `$2` the reference, which only has to be present.
pub(super) const PLATFORM: &str = include_str!("platform.sql");

/// The catalogue version, share-locked before anything else the read locks,
/// because a catalogue edit takes it first.
pub(super) const LOCK_CATALOGUE: &str = include_str!("lock_catalogue.sql");

/// The credential [`ENTRY`] will select, share-locked before the entry, because
/// an entry removal and a credential delete take the credential first.
///
/// `$1` owner, `$2` opaque reference, as [`ENTRY`] takes them.
pub(super) const LOCK_ENTRY_CREDENTIAL: &str = include_str!("lock_entry_credential.sql");

/// The credential [`PLATFORM`] will select, share-locked before the default, for
/// the same reason as [`LOCK_ENTRY_CREDENTIAL`]. It takes no parameters.
pub(super) const LOCK_PLATFORM_CREDENTIAL: &str = include_str!("lock_platform_credential.sql");
