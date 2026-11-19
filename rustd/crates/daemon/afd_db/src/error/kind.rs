//! Typed database causes remain separate from their public classification.

use std::time::Duration;

/// What actually went wrong. Private so a new variant is not a breaking change.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    #[error("{knob} requires a whole number")]
    InvalidPoolNumber {
        knob: Box<str>,
        #[source]
        source: std::num::ParseIntError,
    },
    #[error("database pool limits failed validation: {}", fields(.source))]
    InvalidPoolBudget {
        #[source]
        source: garde::Report,
    },

    #[error("{knob} is not set")]
    MissingDatabaseUrl { knob: &'static str },

    #[error("{knob} is not a Postgres connection URL")]
    InvalidDatabaseUrl {
        knob: &'static str,
        // Boxed because `sqlx::Error` is large and this variant is the cold
        // path; an unboxed one would widen every `Result<_>` this crate
        // returns, including the ones on the request path.
        #[source]
        source: Box<sqlx::Error>,
    },

    #[error("{knob} must be a postgres:// or postgresql:// URL")]
    InvalidDatabaseUrlScheme { knob: &'static str },

    #[error("{knob} sets {param}={path}, which this process cannot read")]
    TlsCertFileUnreadable {
        knob: &'static str,
        param: &'static str,
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("{knob} must be true, false, 1 or 0")]
    InvalidBoolKnob { knob: &'static str },

    #[error("waited {waited:?} for a {role} connection and the pool had none")]
    PoolCapacity {
        role: &'static str,
        waited: Duration,
    },

    #[error("the {role} datastore did not answer within {waited:?}")]
    DatastoreUnreachable {
        role: &'static str,
        waited: Duration,
    },

    #[error(
        "waited {waited:?} for a {role} connection: the pool held {held} of {ceiling} and \
         could not open another"
    )]
    AcquireStalled {
        role: &'static str,
        waited: Duration,
        held: u32,
        ceiling: u32,
    },

    #[error("the {role} datastore is unreachable")]
    DatastoreUnavailable {
        role: &'static str,
        #[source]
        source: sqlx::Error,
    },

    #[error("statement failed during {context}")]
    Query {
        context: &'static str,
        #[source]
        source: sqlx::Error,
    },

    #[error("migration {version} failed to apply")]
    MigrationFailed {
        version: i64,
        #[source]
        source: sqlx::Error,
    },

    #[error("the migration lock was held by another session for {waited:?}")]
    MigrationLockUnavailable { waited: Duration },

    #[error("the ledger records version {found}, which this binary does not know")]
    MigrationSchemaAhead { found: i64 },

    #[error("migration {version} was applied, and its file has changed since")]
    MigrationEdited { version: i64 },

    #[error("the migrator refused to run")]
    MigrationRefused {
        #[source]
        source: sqlx::migrate::MigrateError,
    },
}

fn fields(report: &garde::Report) -> String {
    report
        .iter()
        .map(|(path, _error)| path.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}
