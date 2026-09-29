//! What a lane's own machinery refused, as distinct from the path it drives.
//!
//! A lane counts what Postgres executed through the server's statement
//! counter, and the lease drain also composes the daemon's whole lease plane
//! over a staged platform credential and reads its answers back. Those can
//! fail in ways that are neither the path under measurement nor a datastore
//! outage: the counter, the credential the drain seals, and a lease answer it
//! has to read. They live in their own type because the crate's error enum
//! reached its file cap, and they compose into it through `?` like every other
//! source.

/// A refusal raised by a lane's counter, or by the drain's staging or readback.
#[derive(Debug, thiserror::Error)]
pub enum LaneFault {
    /// The statement counter would not answer.
    ///
    /// Almost always a Postgres started without `pg_stat_statements` in
    /// `shared_preload_libraries`: the extension's view refuses to read
    /// until the library is loaded, and that happens only at server start.
    #[error(
        "the statement counter would not answer: is pg_stat_statements preloaded? \
         the compose postgres preloads it, so recreate it with make _ensure-test-infra"
    )]
    StatementsUnreadable {
        /// What Postgres said.
        source: sqlx::Error,
    },

    /// Another platform default already holds the provider the drain stages.
    ///
    /// Refused rather than overwritten: the row is deployment-wide, and
    /// repointing it would change what every other fleet on this database
    /// resolves to.
    #[error(
        "a platform default for {provider} already exists on this database: \
         reset the rig (make _reset-test-db) before draining"
    )]
    PlatformDefaultHeld {
        /// The provider whose default is taken.
        provider: &'static str,
    },

    /// The drain's platform credential would not seal.
    #[error("the drain's platform credential would not seal")]
    CredentialUnsealable {
        /// What `afd_crypto` refused.
        #[from]
        source: afd_crypto::error::Error,
    },

    /// A lease answer that does not read as the wire's lease response.
    #[error("a lease answer would not parse as a lease response")]
    LeaseUnreadable {
        /// Where serde gave up.
        source: serde_json::Error,
    },
}
