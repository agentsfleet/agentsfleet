//! The one error type this crate returns, and the wire code each failure maps to.
//!
//! Same shape as [`afd_db::Error`] (`M-ERRORS-CANONICAL-STRUCTS`): a struct
//! carrying a captured backtrace and a private kind, with `is_*` accessors
//! rather than a public enum, so a new internal failure mode is not a breaking
//! change for anyone matching on it.
//!
//! # What the Zig control plane spells instead
//!
//! `service.zig`, `service_billing.zig` and `assign.zig` have no error type.
//! They answer `?T` and log on the way past — `assign.select` catches every
//! failure eight frames down, writes a `warn`, and returns `null`, which the
//! lease handler cannot tell apart from "there is genuinely no work". A
//! transient Postgres blip and an idle fleet reach the caller as the same
//! value, so the only thing that knows the difference is a log line nobody
//! reads at request time.
//!
//! That is RULE ECL's failure exactly, and it is not fixed by adding a log. It
//! is fixed by making the difference a TYPE the caller has to handle, which is
//! what this file is for. The lease verb still answers no-work-with-backoff on
//! a transient failure — that is Zig parity and it stays — but it does so at
//! ONE place that says so, instead of at eight `catch` sites that each decided
//! it privately.

use std::backtrace::Backtrace;
use std::fmt::{self, Display, Formatter};

use afd_core::error_code::{self, ErrorCode};

/// The result every fallible function in this crate returns.
///
/// One alias per crate, defaulted to that crate's own [`Error`] — the shape
/// `core_api` has run in production on for years, and the one bun uses
/// (`pub type Result<T, E = Error>`). The default parameter is what lets the
/// few functions answering with a foreign error keep the same spelling.
///
/// The point is not brevity. It is that a reader never has to check WHICH
/// error a signature returns to know it is this crate's.
pub type Result<T, E = Error> = core::result::Result<T, E>;

/// `register.zig`'s refusal when `host_id` is absent or too long.
///
/// Client-visible, so it is pinned byte-for-byte: parity in this milestone is
/// behavioural, and what a caller reads is behaviour (RULE UFS).
pub const DETAIL_HOST_ID_BOUNDS: &str = "host_id must be 1-256 chars";

/// `register.zig`'s refusal for a malformed registry allowlist entry.
pub const DETAIL_REGISTRY_ALLOWLIST: &str = "registry_allowlist entries must be host[:port] names";

/// `self.zig`'s refusal when the token authenticated and the row is gone.
pub const DETAIL_RUNNER_NOT_FOUND: &str = "runner not found";

/// A runner control-plane operation failed.
///
/// One pointer wide, for the reason `afd_db::Error` is: the largest kind
/// carries a `sqlx::Error` at 128 bytes, and this type is the `Err` of
/// `Result`s the request path returns. Boxing keeps the success path — the one
/// that runs — the size of what it actually carries
/// (`clippy::result_large_err`).
#[derive(Debug)]
pub struct Error {
    inner: Box<Inner>,
}

#[derive(Debug)]
struct Inner {
    kind: ErrorKind,
    backtrace: Backtrace,
}

/// What actually went wrong. Private so a new variant is not a breaking change.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    #[error("the datastore backing the runner plane would not answer")]
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

    #[error("the presented runner token proved a row that no longer exists")]
    RunnerVanished,

    #[error("a {table} row holds a value this daemon cannot read: {column}")]
    RowMalformed {
        table: &'static str,
        column: &'static str,
        #[source]
        source: afd_core::error::Error,
    },

    #[error("{detail}")]
    Rejected { detail: &'static str },

    #[error("an identifier could not be minted from the current instant")]
    Identity {
        #[from]
        source: afd_core::error::Error,
    },

    #[error("could not draw the entropy a credential is minted from")]
    Entropy {
        #[source]
        source: afd_crypto::error::Error,
    },
}

impl Error {
    pub(crate) fn new(kind: ErrorKind) -> Self {
        Self {
            inner: Box::new(Inner {
                kind,
                backtrace: Backtrace::capture(),
            }),
        }
    }

    /// Whether the datastore could not be reached at all.
    ///
    /// The question the runner plane turns on: a caller answering this `true`
    /// must report a transport failure, never an authentication or a validation
    /// one, because the runner client counts rejections toward a
    /// self-termination ceiling and resets that counter on transport failures
    /// (RULE ECL, and `docs/AUTH.md` §Runner token).
    #[must_use]
    pub fn is_datastore_unavailable(&self) -> bool {
        matches!(self.inner.kind, ErrorKind::Datastore { .. })
    }

    /// Whether the caller sent something this plane will not accept.
    ///
    /// The only kind whose message is written FOR the caller; every other kind
    /// answers a fixed registry sentence and keeps its detail in the log.
    #[must_use]
    pub fn is_rejected(&self) -> bool {
        matches!(self.inner.kind, ErrorKind::Rejected { .. })
    }

    /// Whether an authenticated runner's row has since disappeared.
    ///
    /// Answered separately from a rejection because the remedy differs: the
    /// token is real and the enrolment is gone, so the host must be re-enrolled
    /// rather than retried.
    #[must_use]
    pub fn is_runner_vanished(&self) -> bool {
        matches!(self.inner.kind, ErrorKind::RunnerVanished)
    }

    /// The registry code this failure answers with.
    ///
    /// Exhaustive, so a new kind fails the build until it is given one — the
    /// same device `afd_auth::Error::code` uses, applied to the pairing the Zig
    /// handlers restate at every `hx.fail` call site.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self.inner.kind {
            ErrorKind::Datastore { .. } => error_code::INTERNAL_DB_UNAVAILABLE,
            ErrorKind::Query { .. } | ErrorKind::RowMalformed { .. } => {
                error_code::INTERNAL_DB_QUERY
            }
            ErrorKind::RunnerVanished => error_code::RUN_INVALID_RUNNER_TOKEN,
            ErrorKind::Identity { .. } => error_code::UUIDV7_INVALID_ID_SHAPE,
            ErrorKind::Rejected { .. } => error_code::INVALID_REQUEST,
            ErrorKind::Entropy { .. } => error_code::INTERNAL_OPERATION_FAILED,
        }
    }

    /// The sentence the caller is told.
    ///
    /// A rejection quotes its own detail, because the caller can act on it —
    /// that is the whole reason the kind exists. Everything else answers the
    /// registry's fixed sentence for its code: an internal failure that quotes
    /// its cause is an internal failure leaking its cause to whoever provoked
    /// it, and `afd_core::problem` already holds the safe wording.
    #[must_use]
    pub const fn detail(&self) -> Option<&'static str> {
        match self.inner.kind {
            ErrorKind::Rejected { detail } => Some(detail),
            ErrorKind::RunnerVanished => Some(DETAIL_RUNNER_NOT_FOUND),
            ErrorKind::Datastore { .. }
            | ErrorKind::Query { .. }
            | ErrorKind::RowMalformed { .. }
            | ErrorKind::Identity { .. }
            | ErrorKind::Entropy { .. } => None,
        }
    }

    /// The backtrace captured when this error was constructed.
    ///
    /// Empty unless `RUST_BACKTRACE` asked for one — capturing is opt-in, so
    /// the common path costs a few instructions rather than microseconds.
    pub fn backtrace(&self) -> &Backtrace {
        &self.inner.backtrace
    }
}

/// Refuses a request the caller can correct, quoting the Zig detail verbatim.
pub(crate) fn rejected(detail: &'static str) -> Error {
    Error::new(ErrorKind::Rejected { detail })
}

/// Reports a statement that reached Postgres and was refused.
///
/// `map_err` that ADDS context the call site alone knows — which statement was
/// running — and nothing else. The `sqlx::Error` rides through as `#[source]`,
/// so the chain a fatal renderer walks stays intact (`RUST_ERROR_STANDARD`
/// rule 3).
pub(crate) fn query(context: &'static str) -> impl Fn(sqlx::Error) -> Error {
    move |source| Error::new(ErrorKind::Query { context, source })
}

/// Reports a column whose stored value is not a shape this daemon can read.
pub(crate) fn row_malformed(
    table: &'static str,
    column: &'static str,
) -> impl Fn(afd_core::error::Error) -> Error {
    move |source| {
        Error::new(ErrorKind::RowMalformed {
            table,
            column,
            source,
        })
    }
}

/// A pool with nothing to give, or a datastore that is gone.
///
/// `#[from]`, so `?` lifts an `afd_db::Error` with no conversion at the call
/// site (`RUST_ERROR_STANDARD` rule 2).
impl From<afd_db::Error> for Error {
    fn from(source: afd_db::Error) -> Self {
        Self::new(ErrorKind::Datastore { source })
    }
}

/// An identifier could not be minted — the instant is unrepresentable.
///
/// `#[from]` on the KIND, lifted here, so `?` carries a `Uuid7::encode` failure
/// with no conversion at the call site. `RowMalformed` wraps the same foreign
/// type but keeps `#[source]` and its own builder, because a column that will
/// not parse needs the table and column names a bare conversion cannot supply —
/// and because two `#[from]` for one type is two `From` impls for one pair.
impl From<afd_core::error::Error> for Error {
    fn from(source: afd_core::error::Error) -> Self {
        Self::new(ErrorKind::Identity { source })
    }
}

/// The host could not produce the random bytes a credential is built from.
impl From<afd_crypto::error::Error> for Error {
    fn from(source: afd_crypto::error::Error) -> Self {
        Self::new(ErrorKind::Entropy { source })
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
    /// `Display` already renders the kind's message, so returning `&self.kind`
    /// would make a chain walker print the same sentence twice before reaching
    /// anything new. The kind is not a CAUSE of this error, it IS this error;
    /// what caused it is whatever the kind wraps (`RUST_ERROR_STANDARD` rule 4).
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        std::error::Error::source(&self.inner.kind)
    }
}
