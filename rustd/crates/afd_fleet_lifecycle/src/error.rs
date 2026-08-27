//! The one error type this crate returns, and what each failure tells a caller.
//!
//! Same shape as [`afd_db::Error`] and `afd_tenant::Error`
//! (`M-ERRORS-CANONICAL-STRUCTS`, with the workspace's declared divergence): a
//! struct carrying a captured backtrace and a private kind, with `is_*`
//! accessors rather than a public enum, so a new failure mode is not a breaking
//! change for anybody matching on it.
//!
//! # One table, not two
//!
//! [`Error::answer`] returns the registry code AND the sentence together, and
//! both public accessors read from it. The Zig handlers spell
//! `hx.fail(code, detail)` at each call site with nothing relating the two, so
//! two handlers can describe one failure differently and both compile. Here a
//! kind cannot take its code and its sentence from different places.
//!
//! # Where a failure is raised
//!
//! A variant carrying no data needs no constructor — `ErrorKind::NotFound.into()`
//! names what a `fn not_found()` would. The kinds that CARRY something keep a
//! raiser, and those live in [`raise`] beside the `From` impls that lift a
//! foreign error into its variant.

use std::backtrace::Backtrace;
use std::fmt::{self, Display, Formatter};

use afd_core::error_code::{self, ErrorCode};

pub mod detail;
mod raise;

pub(crate) use self::raise::{query, row_malformed, skill, source_stale};

/// The result every fallible function in this crate returns.
///
/// One alias per crate, defaulted to this crate's own [`Error`], so a reader
/// never has to check WHICH error a signature returns to know it is this one.
pub type Result<T, E = Error> = core::result::Result<T, E>;

/// A fleet-lifecycle failure, with the backtrace of where it was raised.
#[derive(Debug)]
pub struct Error {
    inner: Box<Inner>,
}

#[derive(Debug)]
struct Inner {
    kind: ErrorKind,
    backtrace: Backtrace,
}

/// What actually went wrong. Crate-visible so a raise site can name the variant.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    #[error("the datastore backing the fleet lifecycle would not answer")]
    Datastore {
        #[source]
        source: afd_db::Error,
    },

    #[error("the queue backing the fleet lifecycle would not answer")]
    Queue {
        #[source]
        source: afd_redis::Error,
    },

    #[error("statement failed during {context}")]
    Query {
        context: &'static str,
        #[source]
        source: sqlx::Error,
    },

    #[error("core.fleets.{column} holds {stored}, which this build does not know")]
    RowMalformed {
        column: &'static str,
        /// The stored bytes, so the log names what a newer daemon wrote.
        stored: Box<str>,
    },

    #[error("an identifier could not be minted from the current instant")]
    Mint {
        #[source]
        source: afd_core::error::Error,
    },

    #[error("could not draw the entropy a fleet identifier is minted from")]
    Entropy {
        #[source]
        source: afd_crypto::error::Error,
    },

    #[error("the authored TRIGGER.md is not a configuration this daemon can store")]
    Config {
        #[source]
        source: afd_fleet_runtime::Error,
    },

    #[error("the authored SKILL.md frontmatter is not one this daemon can store")]
    Skill {
        #[source]
        source: afd_fleet_runtime::Error,
    },

    #[error("the authored SKILL.md is empty or past its length bound")]
    SkillRejected,

    #[error("the authored TRIGGER.md is empty or past its length bound")]
    TriggerRejected,

    #[error("SKILL.md and TRIGGER.md name different fleets")]
    NameMismatch,

    #[error("this workspace already holds a fleet under the chosen name")]
    NameExists,

    #[error("no fleet with that id lives in this workspace")]
    NotFound,

    #[error("the fleet's current status does not permit that transition")]
    TransitionRefused,

    #[error("a fleet must be killed before it can be purged")]
    MustKillFirst,

    #[error("the caller's If-Match names a source version the row has moved past")]
    SourceStale {
        /// The tag the row holds now, so an editor re-applies in one round trip.
        current: Box<str>,
    },

    #[error("the install could not be finished, and the row was rolled back")]
    InstallRolledBack,

    #[error("no installable library entry matches that id in this workspace")]
    LibraryEntryMissing,

    #[error("the placement tags are outside the bounds a lease can match on")]
    RequiredTagsInvalid,
}

impl Error {
    /// The code and the sentence, decided together — see the module note.
    const fn answer(&self) -> (ErrorCode, &'static str) {
        match self.inner.kind {
            // One code for both stores — a caller acts identically on either —
            // and two sentences, so an operator reading a 503 body does not
            // have to open the log to learn which datastore is down.
            ErrorKind::Datastore { .. } => (
                error_code::INTERNAL_DB_UNAVAILABLE,
                detail::DATABASE_UNAVAILABLE,
            ),
            ErrorKind::Queue { .. } => (
                error_code::INTERNAL_DB_UNAVAILABLE,
                detail::QUEUE_UNAVAILABLE,
            ),
            // Four internal failures, one fixed sentence: a failed statement, an
            // unreadable row, a clock that cannot name an instant and a host
            // short of entropy are all this process's problem, and naming which
            // would leak the cause to whoever provoked it.
            ErrorKind::Query { .. }
            | ErrorKind::RowMalformed { .. }
            | ErrorKind::Mint { .. }
            | ErrorKind::Entropy { .. } => (error_code::INTERNAL_DB_QUERY, detail::DATABASE_ERROR),
            // A `TRIGGER.md` that failed to parse and one never parsed because
            // it is empty or oversized answer alike: the caller opens the same
            // file either way.
            ErrorKind::Config { .. } | ErrorKind::TriggerRejected => (
                error_code::AGENTSFLEET_INVALID_CONFIG,
                detail::INVALID_CONFIG,
            ),
            // The same code as an unusable `TRIGGER.md`, and a different
            // sentence: both mean "this bundle will not install", and the
            // sentence is what says which of its two documents to open.
            // A document never parsed because it is empty or past its bound
            // answers exactly as one that parsed and failed: the caller's remedy
            // is the same file either way, and a separate code would make a
            // client branch on a distinction it cannot act on.
            ErrorKind::Skill { .. } | ErrorKind::SkillRejected => (
                error_code::AGENTSFLEET_INVALID_CONFIG,
                detail::SKILL_INVALID,
            ),
            ErrorKind::NameMismatch => {
                (error_code::AGENTSFLEET_NAME_MISMATCH, detail::NAME_MISMATCH)
            }
            ErrorKind::NameExists => (error_code::AGENTSFLEET_NAME_EXISTS, detail::NAME_EXISTS),
            ErrorKind::NotFound => (error_code::AGENTSFLEET_NOT_FOUND, detail::NOT_FOUND),
            // One code for two refusals a client acts identically on: a
            // transition the machine rejects, and a purge of a fleet nobody
            // killed first. The sentence says which, and what to do about it.
            ErrorKind::TransitionRefused => (
                error_code::AGENTSFLEET_ALREADY_TERMINAL,
                detail::TRANSITION_REFUSED,
            ),
            ErrorKind::MustKillFirst => (
                error_code::AGENTSFLEET_ALREADY_TERMINAL,
                detail::MUST_KILL_FIRST,
            ),
            ErrorKind::SourceStale { .. } => {
                (error_code::AGENTSFLEET_SOURCE_STALE, detail::SOURCE_STALE)
            }
            ErrorKind::InstallRolledBack => (
                error_code::AGENTSFLEET_INSTALL_ROLLED_BACK,
                detail::INSTALL_ROLLED_BACK,
            ),
            ErrorKind::LibraryEntryMissing => (
                error_code::FLEET_BUNDLE_NOT_FOUND,
                detail::LIBRARY_ENTRY_MISSING,
            ),
            ErrorKind::RequiredTagsInvalid => {
                (error_code::INVALID_REQUEST, detail::REQUIRED_TAGS_INVALID)
            }
        }
    }

    /// The registry code this failure answers with.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        self.answer().0
    }

    /// The sentence the caller is told.
    #[must_use]
    pub const fn detail(&self) -> &'static str {
        self.answer().1
    }

    /// Whether the datastore behind this crate could not be reached.
    ///
    /// The question the HTTP edge turns on: an outage is this instance's
    /// problem to report as a 503, where every other failure here is the
    /// caller's to correct (RULE ECL).
    #[must_use]
    pub const fn is_datastore_unavailable(&self) -> bool {
        matches!(
            self.inner.kind,
            ErrorKind::Datastore { .. } | ErrorKind::Queue { .. }
        )
    }

    /// The tag the row holds now, when this is a stale-source refusal.
    ///
    /// `Some` for exactly one kind. The 412's body carries it so an editor
    /// re-applies without a second round trip to learn what it should have sent.
    #[must_use]
    pub fn stale_tag(&self) -> Option<&str> {
        match &self.inner.kind {
            ErrorKind::SourceStale { current } => Some(current),
            _not_stale => None,
        }
    }

    /// The backtrace captured at the raise site, empty unless `RUST_BACKTRACE`
    /// asked for one — capturing is opt-in, so the common path stays cheap.
    pub fn backtrace(&self) -> &Backtrace {
        &self.inner.backtrace
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code().as_str(), self.inner.kind)?;
        if self.inner.backtrace.status() == std::backtrace::BacktraceStatus::Captured {
            write!(f, "\n{}", self.inner.backtrace)?;
        }
        Ok(())
    }
}

impl std::error::Error for Error {
    /// The failure beneath this one, skipping our own kind.
    ///
    /// `Display` already renders the kind's message, so returning the kind
    /// would make a chain walker print the same sentence twice before reaching
    /// anything new. The kind is not a CAUSE of this error, it IS this error
    /// (`RUST_ERROR_STANDARD` rule 4).
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        std::error::Error::source(&self.inner.kind)
    }
}

/// The one place a kind becomes an error, so every raise captures a backtrace.
impl From<ErrorKind> for Error {
    fn from(kind: ErrorKind) -> Self {
        Self {
            inner: Box::new(Inner {
                kind,
                backtrace: Backtrace::capture(),
            }),
        }
    }
}
