//! What this crate refuses, and what it reports.
//!
//! One error type with `pub type Result<T, E = Error>` beside it, composed
//! with `#[from]` so `?` lifts a datastore failure without restating it.
//! Nothing here maps another crate's error to a string: the `source()` chain
//! is what an operator follows from "the enrolment did not land" to the
//! Postgres detail that says why.

use afd_core::error_code::{self, ErrorCode};

/// The sentence an out-of-bounds host identifier earns.
pub const DETAIL_HOST_ID_BOUNDS: &str = "host_id must be 1-256 chars";

/// The sentence a malformed registry allowlist earns.
pub const DETAIL_REGISTRY_ALLOWLIST: &str = "registry_allowlist entries must be host[:port] names";

/// The sentence a vanished runner earns.
pub const DETAIL_RUNNER_NOT_FOUND: &str = "runner not found";

/// `runner_patch.zig`'s refusal when a terminal runner cannot collect an ask.
pub const DETAIL_SELFTEST_REFUSED: &str = "revoked runners cannot be asked to self-test";

/// Every way enrolling, proving or sweeping a runner can fail.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A statement would not run.
    #[error("the runner store could not {context}")]
    Query {
        /// What was being attempted, for the operator's log line.
        context: &'static str,
        /// The Postgres failure underneath.
        #[source]
        source: sqlx::Error,
    },

    /// A stored value is not a shape this daemon can read.
    #[error("{table}.{column} holds a value this build cannot read")]
    RowMalformed {
        /// Which table the column belongs to.
        table: &'static str,
        /// Which column refused.
        column: &'static str,
        /// The parse failure underneath.
        #[source]
        source: afd_core::error::Error,
    },

    /// An authenticated runner's row has since disappeared.
    ///
    /// Its own variant rather than a rejection because the remedy differs: the
    /// token is real and the enrolment is gone, so the host must be
    /// re-enrolled rather than retried.
    #[error("the presented runner token proved a row that no longer exists")]
    RunnerVanished,

    /// No runner row matches the id an operator addressed.
    ///
    /// Distinct from [`Error::RunnerVanished`], which is the same absence
    /// reached through a runner's OWN authenticated token: that one says the
    /// host must re-enrol, and this one says the operator typed an id nothing
    /// holds. They answer different registry codes for that reason.
    #[error("no runner matches the operator-supplied id")]
    RunnerNotFound,

    /// A revoked runner was asked to collect a self-test.
    ///
    /// Revocation is terminal, so the ask can never be collected — a refusal
    /// rather than a retry.
    #[error("a revoked runner cannot collect a self-test request")]
    SelftestRefused,

    /// A runner row holds an `admin_state` outside the wire enum.
    #[error("a runner row holds an unknown admin state")]
    AdminStateMalformed,

    /// Stored JSONB did not survive decoding into its wire value.
    #[error("{table}.{column} holds JSON this build cannot read")]
    StoredJson {
        /// Which table the column belongs to.
        table: &'static str,
        /// Which column refused.
        column: &'static str,
        /// The decode failure underneath.
        #[source]
        source: serde_json::Error,
    },

    /// The caller sent something this plane will not accept.
    ///
    /// The only variant whose sentence is written FOR the caller; every other
    /// answers a fixed registry sentence and keeps its detail in the log.
    #[error("{detail}")]
    Rejected {
        /// The sentence the caller is told.
        detail: &'static str,
    },

    /// The entropy a runner credential is minted from could not be drawn.
    #[error(transparent)]
    Entropy {
        /// The entropy failure underneath.
        #[from]
        source: afd_crypto::error::Error,
    },

    /// An identifier could not be minted from the current instant.
    #[error(transparent)]
    Identifier {
        /// The identifier failure underneath.
        #[from]
        source: afd_core::error::Error,
    },

    /// A vault row's decrypted body is not the shape a credential must be.
    ///
    /// Carries no source, deliberately: WHICH check refused is an oracle, and
    /// the operator-facing fact — this stored secret cannot repair this run —
    /// is the whole of what the variant says.
    #[error("a stored credential's body is not a readable shape")]
    VaultDataInvalid,

    /// The queue would not take the repaired delivery.
    #[error("the fleet's stream could not be reached")]
    Queue {
        /// The queue failure underneath.
        #[from]
        source: afd_redis::Error,
    },

    /// The pool would not give a connection.
    #[error("the runner store's datastore is unavailable")]
    Datastore {
        /// The pool failure underneath.
        #[from]
        source: afd_db::Error,
    },
}

impl Error {
    /// The sentence a caller is told.
    #[must_use]
    pub const fn detail(&self) -> &'static str {
        match self {
            Self::Rejected { detail } => detail,
            Self::RunnerVanished | Self::RunnerNotFound => DETAIL_RUNNER_NOT_FOUND,
            Self::SelftestRefused => DETAIL_SELFTEST_REFUSED,
            // The four datastore-shaped failures — a statement that reached
            // Postgres, and three rows this build cannot read — all answer
            // `INTERNAL_DB_QUERY`, so they share the datastore sentence. The
            // rest keep this plane's generic one: a sentence has to be true of
            // every variant mapped to it, and naming one verb makes it false
            // for the others.
            Self::Query { .. }
            | Self::RowMalformed { .. }
            | Self::StoredJson { .. }
            | Self::AdminStateMalformed => DETAIL_DATABASE_ERROR,
            Self::Entropy { .. } | Self::Identifier { .. } | Self::VaultDataInvalid => {
                DETAIL_OPERATION_FAILED
            }
            Self::Datastore { .. } | Self::Queue { .. } => DETAIL_UNAVAILABLE,
        }
    }

    /// Whether the datastore behind this crate could not be reached.
    #[must_use]
    pub const fn is_datastore_unavailable(&self) -> bool {
        matches!(self, Self::Datastore { .. } | Self::Queue { .. })
    }

    /// Whether the caller sent something this plane will not accept.
    #[must_use]
    pub const fn is_rejected(&self) -> bool {
        matches!(self, Self::Rejected { .. })
    }

    /// Whether an authenticated runner's row has since disappeared.
    #[must_use]
    pub const fn is_runner_vanished(&self) -> bool {
        matches!(self, Self::RunnerVanished)
    }

    /// The registry code a caller is refused with.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::Rejected { .. } => error_code::INVALID_REQUEST,
            Self::RunnerVanished => error_code::RUN_INVALID_RUNNER_TOKEN,
            Self::RunnerNotFound => error_code::RUNNER_NOT_FOUND,
            Self::SelftestRefused => error_code::RUN_SELFTEST_REFUSED,
            Self::Query { .. }
            | Self::RowMalformed { .. }
            | Self::StoredJson { .. }
            // A stored `admin_state` outside the wire enum is a ROW this build
            // cannot read, not an operator's request being wrong — same family
            // as a column that will not decode.
            | Self::AdminStateMalformed => error_code::INTERNAL_DB_QUERY,
            // A daemon whose clock cannot name an instant, and a host that
            // cannot draw random bytes, are both THIS process failing — never
            // the caller's request being wrong.
            Self::Entropy { .. } | Self::Identifier { .. } => error_code::INTERNAL_OPERATION_FAILED,
            // The body's SHAPE is a fact the operator who stored it can act
            // on, so it answers the vault's own code rather than the internal
            // family — the split `crypto_store.zig` and `vault.zig` draw.
            Self::VaultDataInvalid => error_code::VAULT_DATA_INVALID,
            Self::Datastore { .. } | Self::Queue { .. } => error_code::INTERNAL_DB_UNAVAILABLE,
        }
    }
}

/// The sentence a statement that would not run earns.
///
/// Names the DATASTORE rather than the verb, because every verb in this crate
/// reaches the same one and a caller cannot act on which of them was running.
/// The sentence a failure carries has to be true of every variant mapped to it
/// — `detail()` maps four here — and a verb-shaped sentence is true of at most
/// one.
pub const DETAIL_DATABASE_ERROR: &str = "Database error";

/// The sentence this plane's non-statement failures earn.
///
/// Entropy that could not be drawn, an instant that could not name an id, and
/// a stored credential body that is not a readable shape. It reads "runner"
/// rather than "registration" because this crate is the runner plane, not the
/// enrolment verb: it also serves the operator views, so a sentence naming one
/// of its verbs would be wrong for the rest.
const DETAIL_OPERATION_FAILED: &str = "The runner operation could not be completed";

/// The sentence an unreachable datastore earns.
const DETAIL_UNAVAILABLE: &str = "Database unavailable";

/// This crate's result, defaulting to its own error.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Reports a statement that would not run, naming what it was doing.
pub(crate) fn query(context: &'static str) -> impl Fn(sqlx::Error) -> Error {
    move |source| Error::Query { context, source }
}

/// Reports a stored value this build cannot read, naming table and column.
pub(crate) fn row_malformed(
    table: &'static str,
    column: &'static str,
) -> impl Fn(afd_core::error::Error) -> Error {
    move |source| Error::RowMalformed {
        table,
        column,
        source,
    }
}

/// Refuses a caller, naming why in their language.
pub(crate) const fn rejected(detail: &'static str) -> Error {
    Error::Rejected { detail }
}

/// Reports a stored credential whose decrypted body is not a readable shape.
pub(crate) const fn vault_data_invalid() -> Error {
    Error::VaultDataInvalid
}

/// Reports an operator request addressed to no runner row.
pub(crate) const fn runner_not_found() -> Error {
    Error::RunnerNotFound
}

/// Refuses a self-test ask that a revoked runner can never collect.
pub(crate) const fn selftest_refused() -> Error {
    Error::SelftestRefused
}

/// Reports a runner row whose administrative state is outside the wire enum.
pub(crate) const fn admin_state_malformed() -> Error {
    Error::AdminStateMalformed
}

/// Reports JSONB text that did not survive decoding into its wire value.
pub(crate) fn stored_json(
    table: &'static str,
    column: &'static str,
) -> impl Fn(serde_json::Error) -> Error {
    move |source| Error::StoredJson {
        table,
        column,
        source,
    }
}
