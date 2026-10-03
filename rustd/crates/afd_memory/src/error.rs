//! The one error type this crate returns, and what each failure tells a caller.
//!
//! The memory operator surface answers `UZ-MEM-*` codes with sentences pinned
//! to the retired daemon's `memory/handler.zig`; the runner plane's verbs answer
//! the internal database codes they always did. Both are decided in one table,
//! [`Error::answer`], so a new kind fails the build until it has both.

use afd_core::error_code::{self, ErrorCode};

pub mod detail;
mod raise;

pub(crate) use self::raise::{entry_not_found, fleet_not_found, moving, query, unavailable};

/// The result every fallible function in this crate returns.
pub type Result<T, E = Error> = core::result::Result<T, E>;

afd_core::error_shell!(
    /// A memory failure, with the backtrace of where it was raised.
    pub struct Error(ErrorKind);
);

/// What actually went wrong. Crate-visible so a raise site can name the variant.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    #[error("the datastore holding memory would not answer")]
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

    #[error("the durable memory store would not answer: {detail}")]
    Unavailable {
        detail: &'static str,
        #[source]
        source: sqlx::Error,
    },

    #[error("an entry identifier could not be minted")]
    Identifier {
        #[source]
        source: afd_crypto::error::Error,
    },

    #[error("a stored entry names a writer that is not an identifier")]
    Writer {
        #[source]
        source: afd_core::error::Error,
    },

    #[error("the fleet a memory request names is not this workspace's")]
    FleetNotFound,

    #[error("the fleet is holding no memory entry under that key")]
    EntryNotFound,

    /// A stand-in store refused the call. Only a suite's store raises it; a
    /// vendor store gains its own kinds when it lands.
    #[cfg(feature = "test-util")]
    #[error("the {store} memory store refused the call")]
    Refused { store: &'static str },

    /// The workspace's memory is being copied to another store.
    #[error("the workspace's memory is moving to the {store} store")]
    Moving { store: &'static str },
}

impl Error {
    /// The code and the sentence, decided together.
    fn answer(&self) -> (ErrorCode, &'static str) {
        match self.kind() {
            ErrorKind::Datastore { .. } => (
                error_code::INTERNAL_DB_UNAVAILABLE,
                detail::DATABASE_UNAVAILABLE,
            ),
            ErrorKind::Query { .. } | ErrorKind::Writer { .. } => {
                (error_code::INTERNAL_DB_QUERY, detail::DATABASE_ERROR)
            }
            ErrorKind::Identifier { .. } => (
                error_code::INTERNAL_OPERATION_FAILED,
                detail::OPERATION_FAILED,
            ),
            // One code, four sentences: each names the OPERATION a 503 came
            // from, the only fact a reader of one on this surface can act on.
            ErrorKind::Unavailable { detail, .. } => (error_code::MEM_UNAVAILABLE, detail),
            #[cfg(feature = "test-util")]
            ErrorKind::Refused { .. } => (error_code::MEM_UNAVAILABLE, detail::STORE_REFUSED),
            ErrorKind::Moving { .. } => (error_code::MEM_UNAVAILABLE, detail::MOVING),
            ErrorKind::FleetNotFound => (
                error_code::MEM_AGENTSFLEET_NOT_FOUND,
                detail::FLEET_NOT_FOUND,
            ),
            ErrorKind::EntryNotFound => (error_code::MEM_ENTRY_NOT_FOUND, detail::ENTRY_NOT_FOUND),
        }
    }

    /// The registry code this failure answers with.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        self.answer().0
    }

    /// The sentence the caller is told.
    #[must_use]
    pub fn detail(&self) -> &'static str {
        self.answer().1
    }

    /// Whether the datastore could not be reached at all.
    #[must_use]
    pub fn is_datastore_unavailable(&self) -> bool {
        matches!(self.kind(), ErrorKind::Datastore { .. })
    }
}

/// The refusal a store that is not the database answers, for a stand-in store
/// a suite drives a flip against.
#[cfg(feature = "test-util")]
impl Error {
    /// `store` refused the call.
    #[must_use]
    pub fn refused(store: &'static str) -> Self {
        ErrorKind::Refused { store }.into()
    }
}
