//! One error of every kind, for the tests that walk the whole surface.
//!
//! Split from `error.rs` so the type stays under the file cap with room for
//! the next kind; behind `test-util` because production builds have no use
//! for a sampler.

use super::{
    Error, ErrorKind, acquire_stalled, classify_acquire, invalid_bool_knob, query,
    unreachable_datastore,
};
use crate::config::DbRole;

/// One error of every kind, for tests that walk the whole surface.
///
/// The M-TEST-UTIL seam, and the same argument as the mocked entropy in
/// `afd_crypto`: `Display`, `code()` and `source()` are what a human reads
/// while something is already going wrong, and most of these kinds cannot be
/// provoked on demand from a test — a pool does not exhaust itself politely.
/// These are the values the production paths build, constructed directly.
#[must_use]
pub fn one_of_each_kind() -> Vec<(&'static str, Error)> {
    vec![
        (
            "missing url",
            Error::new(ErrorKind::MissingDatabaseUrl {
                knob: DbRole::Default.url_knob(),
            }),
        ),
        (
            "invalid url scheme",
            Error::new(ErrorKind::InvalidDatabaseUrlScheme {
                knob: DbRole::Default.url_knob(),
            }),
        ),
        (
            "unreadable tls cert file",
            Error::new(ErrorKind::TlsCertFileUnreadable {
                knob: DbRole::Migrator.url_knob(),
                param: "sslrootcert",
                path: "/nonexistent/ca.pem".to_owned(),
                source: std::io::Error::from(std::io::ErrorKind::NotFound),
            }),
        ),
        ("invalid bool knob", invalid_bool_knob("MIGRATE_ON_START")),
        (
            "pool capacity",
            Error::new(ErrorKind::PoolCapacity {
                role: DbRole::Api.tag(),
                waited_ms: 2_000,
            }),
        ),
        (
            "datastore unreachable",
            unreachable_datastore(DbRole::Default.tag(), 10_000),
        ),
        (
            "acquire stalled",
            acquire_stalled(DbRole::Api.tag(), 2_000, 5, 20),
        ),
        (
            "datastore unavailable",
            classify_acquire(DbRole::Default.tag(), 2_000, sqlx::Error::PoolClosed),
        ),
        (
            "query",
            query("migrate.ensure_tables", sqlx::Error::PoolClosed),
        ),
        (
            "migration failed",
            Error::new(ErrorKind::MigrationFailed {
                version: 100,
                source: sqlx::Error::PoolClosed,
            }),
        ),
        (
            "lock unavailable",
            Error::new(ErrorKind::MigrationLockUnavailable { waited_ms: 30_000 }),
        ),
        (
            "schema ahead",
            Error::new(ErrorKind::MigrationSchemaAhead { found: 999 }),
        ),
    ]
}
