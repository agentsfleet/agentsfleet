//! What this crate refuses, and what it reports.
//!
//! The hull — the boxed struct, the backtrace, `Display`, `source()` — comes
//! from [`afd_core::error_shell!`]. What is here is the part that varies: the
//! kinds this plane can fail with, the code and sentence each answers, and the
//! raisers that bind data.

use afd_core::error_code::{self, ErrorCode};

/// The result every fallible function in this crate returns.
///
/// One alias per crate, defaulted to this crate's own [`Error`], so a reader
/// never has to check WHICH error a signature returns to know it is this one
/// (`RUST_ERROR_STANDARD` rule 1).
pub type Result<T, E = Error> = core::result::Result<T, E>;

afd_core::error_shell!(
    /// A policy or gate failure, with the backtrace of where it was raised.
    pub struct Error(ErrorKind);
);

/// What actually went wrong. Crate-visible so a raise site can name the variant.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    #[error("the datastore backing the gate plane would not answer")]
    Datastore {
        #[source]
        source: afd_db::Error,
    },

    #[error("statement failed during {context}")]
    Query {
        context: &'static str,
        #[source]
        source: sqlx::Error,
    },

    #[error("the queue backing the gate plane would not answer")]
    Queue {
        #[source]
        source: afd_redis::Error,
    },

    /// The caller sent something this plane will not accept.
    ///
    /// The only variant whose sentence is written FOR the caller; every other
    /// answers a fixed registry sentence and keeps its detail in the log.
    #[error("{detail}")]
    Rejected { detail: &'static str },

    /// A credential the policy assembly had to resolve could not be.
    #[error("the credential plane could not answer for the gate plane")]
    Credential {
        #[source]
        source: afd_credential::Error,
    },

    /// The entropy a gate reference is minted from could not be drawn.
    #[error(transparent)]
    Entropy {
        source: afd_crypto::error::Error,
    },

    /// An identifier could not be minted from the current instant.
    #[error(transparent)]
    Identifier {
        source: afd_core::error::Error,
    },

    /// A money read a gate is priced against failed.
    #[error("the billing store could not answer for the gate plane")]
    Billing {
        #[source]
        source: afd_billing::Error,
    },
}

impl Error {
    /// The registry code this failure answers with.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self.kind() {
            ErrorKind::Datastore { .. } => error_code::INTERNAL_DB_UNAVAILABLE,
            ErrorKind::Query { .. } => error_code::INTERNAL_DB_QUERY,
            ErrorKind::Rejected { .. } => error_code::INVALID_REQUEST,
            // The queue joins the internal family for the registry reason the
            // runner plane's does: the Zig logs `ERR_INTERNAL_OPERATION_FAILED`
            // for every Redis failure it meets, and a new code would fire the
            // ERROR REGISTRY gate over a registry this family does not own.
            // A daemon that cannot draw random bytes or name an instant is THIS
            // process failing, not the caller's request being wrong.
            ErrorKind::Queue { .. } | ErrorKind::Entropy { .. } | ErrorKind::Identifier { .. } => {
                error_code::INTERNAL_OPERATION_FAILED
            }
            // Delegated rather than restated: each plane already decides which
            // of its own failures is an outage and which is a fault, and a
            // second copy of that mapping here is what drifts.
            ErrorKind::Credential { source } => source.code(),
            ErrorKind::Billing { source } => source.code(),
        }
    }

    /// The sentence the caller is told.
    #[must_use]
    pub const fn detail(&self) -> &'static str {
        match self.kind() {
            ErrorKind::Rejected { detail } => detail,
            ErrorKind::Datastore { .. } => DETAIL_UNAVAILABLE,
            ErrorKind::Query { .. }
            | ErrorKind::Queue { .. }
            | ErrorKind::Entropy { .. }
            | ErrorKind::Identifier { .. } => DETAIL_OPERATION_FAILED,
            ErrorKind::Credential { source } => source.detail(),
            ErrorKind::Billing { source } => source.detail(),
        }
    }

    /// Whether the datastore or queue behind this crate could not be reached.
    #[must_use]
    pub const fn is_datastore_unavailable(&self) -> bool {
        match self.kind() {
            ErrorKind::Datastore { .. } => true,
            ErrorKind::Credential { source } => source.is_datastore_unavailable(),
            ErrorKind::Billing { source } => source.is_datastore_unavailable(),
            _served => false,
        }
    }

    /// Whether the caller sent something this plane will not accept.
    #[must_use]
    pub const fn is_rejected(&self) -> bool {
        matches!(self.kind(), ErrorKind::Rejected { .. })
    }

    /// Whether this failure is a stored-configuration fault a human must fix.
    ///
    /// Delegated to the credential plane where it owns the answer: a stored
    /// endpoint the guard refused does not become safe by being retried, while
    /// a datastore that would not answer recovers on its own.
    #[must_use]
    pub const fn is_config_permanent(&self) -> bool {
        match self.kind() {
            ErrorKind::Credential { source } => source.is_config_permanent(),
            _transient => false,
        }
    }
}

/// The sentence an unreachable datastore or queue earns.
const DETAIL_UNAVAILABLE: &str = "Database unavailable";

/// The sentence a statement that would not run earns.
const DETAIL_OPERATION_FAILED: &str = "The operation could not be completed";

/// The sentence a gate whose binding could not be written earns.
pub const DETAIL_GATE_BINDING_UNWRITABLE: &str = "The approval binding could not be recorded";

/// The sentence a gate whose reference could not be written earns.
pub const DETAIL_GATE_REFERENCE_UNWRITABLE: &str = "The approval reference could not be recorded";

afd_core::error_lifts!(Error, ErrorKind:
    afd_db::Error => Datastore,
    afd_redis::Error => Queue,
    afd_credential::Error => Credential,
    afd_billing::Error => Billing,
    afd_crypto::error::Error => Entropy,
    afd_core::error::Error => Identifier,
);

/// Reports a statement that would not run, naming what it was doing.
pub(crate) fn query(context: &'static str) -> impl Fn(sqlx::Error) -> Error {
    move |source| ErrorKind::Query { context, source }.into()
}

/// Refuses a caller, naming why in their language.
pub(crate) fn rejected(detail: &'static str) -> Error {
    ErrorKind::Rejected { detail }.into()
}
