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

pub mod classify;
pub mod detail;

pub use self::detail::{
    DETAIL_CONFIG_UNREADABLE, DETAIL_CREDENTIAL_MISSING, DETAIL_DATABASE_ERROR,
    DETAIL_DATABASE_UNAVAILABLE, DETAIL_EVENT_MALFORMED, DETAIL_GATE_BINDING_UNWRITABLE,
    DETAIL_GATE_REFERENCE_UNWRITABLE, DETAIL_HOST_ID_BOUNDS, DETAIL_PROVIDER_UNRESOLVED,
    DETAIL_QUEUE_UNAVAILABLE, DETAIL_REGISTRATION_FAILED, DETAIL_REGISTRY_ALLOWLIST,
    DETAIL_RUNNER_NOT_FOUND, DETAIL_VAULT_DATA_INVALID,
};

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

    #[error("the queue backing the runner plane would not answer")]
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

    #[error("the presented runner token proved a row that no longer exists")]
    RunnerVanished,

    #[error("a {table} row holds a value this daemon cannot read: {column}")]
    RowMalformed {
        table: &'static str,
        column: &'static str,
        #[source]
        source: afd_core::error::Error,
    },

    #[error("the leased event envelope is missing {field}")]
    Envelope { field: &'static str },

    #[error("{detail}")]
    Rejected { detail: &'static str },

    #[error("an identifier could not be minted from the current instant")]
    Mint {
        #[from]
        source: afd_core::error::Error,
    },

    #[error("could not draw the entropy a credential is minted from")]
    Entropy {
        #[source]
        source: afd_crypto::error::Error,
    },

    #[error("the fleet's stored configuration cannot be read")]
    ConfigUnreadable {
        #[from]
        source: afd_fleet_runtime::Error,
    },

    #[error("a stored provider credential holds no usable {field}")]
    ProviderMalformed { field: &'static str },

    #[error("the tenant's provider selection names a vault row that is not held")]
    ProviderSecretMissing,

    #[error("no active platform provider default is configured")]
    ProviderPlatformKeyMissing,

    #[error("the tenant has no workspace to resolve a credential in")]
    ProviderNoWorkspace,

    #[error("a stored provider endpoint was refused: {reason}")]
    ProviderEndpoint { reason: &'static str },

    #[error("a stored credential envelope would not open")]
    Vault {
        #[source]
        source: afd_crypto::error::Error,
    },

    #[error("a stored credential body is not the JSON object the tool bridge addresses")]
    VaultDataInvalid,

    #[error("the fleet declared a credential this workspace does not hold")]
    CredentialMissing,
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

    /// The backtrace captured when this error was constructed.
    ///
    /// Empty unless `RUST_BACKTRACE` asked for one — capturing is opt-in, so
    /// the common path costs a few instructions rather than microseconds.
    pub fn backtrace(&self) -> &Backtrace {
        &self.inner.backtrace
    }
}

/// Reports a stream entry that does not satisfy the producer's contract.
///
/// `field` is `&'static str` rather than an owned name because every caller
/// passes one of the envelope's own constants — a name that had to be
/// allocated would mean it came from somewhere other than the contract.
pub(crate) fn envelope_field(field: &'static str) -> Error {
    Error::new(ErrorKind::Envelope { field })
}

/// Refuses a request the caller can correct, quoting the Zig detail verbatim.
pub(crate) fn rejected(detail: &'static str) -> Error {
    Error::new(ErrorKind::Rejected { detail })
}

/// Reports a stored provider credential that cannot be read as one.
///
/// `field` names WHICH part is unusable, for the operator's log line. It never
/// reaches the caller — see [`DETAIL_PROVIDER_UNRESOLVED`].
pub(crate) fn provider_malformed(field: &'static str) -> Error {
    Error::new(ErrorKind::ProviderMalformed { field })
}

/// Reports a provider selection naming a vault row that is not held.
pub(crate) fn provider_secret_missing() -> Error {
    Error::new(ErrorKind::ProviderSecretMissing)
}

/// Reports that no operator has set an active platform default.
pub(crate) fn provider_platform_key_missing() -> Error {
    Error::new(ErrorKind::ProviderPlatformKeyMissing)
}

/// Reports a tenant with no workspace to resolve a credential in.
pub(crate) fn provider_no_workspace() -> Error {
    Error::new(ErrorKind::ProviderNoWorkspace)
}

/// Reports a stored endpoint the SSRF guard refused.
///
/// `reason` is the guard's own word for what it refused, never the URL and
/// never the host — the `api_key` sits beside both in the same credential, and a
/// rejection line that quotes the credential is a rejection line that leaks it.
pub(crate) fn provider_endpoint(reason: &'static str) -> Error {
    Error::new(ErrorKind::ProviderEndpoint { reason })
}

/// Reports a stored envelope that is malformed or will not authenticate.
///
/// `map_err` rather than `?`, because the blanket [`From`] for this foreign
/// type already means "the host could not draw entropy" — and an envelope that
/// will not open is not that. This is the carve-out the standard names: a
/// conversion that ADDS the context the call site alone knows, with the cause
/// riding through as `#[source]` so the chain stays intact.
pub(crate) fn vault_open(source: afd_crypto::error::Error) -> Error {
    Error::new(ErrorKind::Vault { source })
}

/// Reports a stored credential body that is not an addressable object.
pub(crate) fn vault_data_invalid() -> Error {
    Error::new(ErrorKind::VaultDataInvalid)
}

/// Reports a declared credential with no vault row.
///
/// The log line — with the workspace and the name an operator needs — is
/// written at the call site, which is the only place that holds them.
pub(crate) fn credential_missing() -> Error {
    Error::new(ErrorKind::CredentialMissing)
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

/// The queue would not answer.
///
/// A separate variant from [`ErrorKind::Datastore`] because the two fail
/// independently and a runner reads them the same way — back off and re-poll —
/// only when the code says which one went down. Folding Redis into the Postgres
/// variant would page whoever owns the wrong datastore.
impl From<afd_redis::Error> for Error {
    fn from(source: afd_redis::Error) -> Self {
        Self::new(ErrorKind::Queue { source })
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
        Self::new(ErrorKind::Mint { source })
    }
}

/// The host could not produce the random bytes a credential is built from.
/// `#[from]` on the KIND, lifted here for the reason the two impls above are:
/// a fleet whose stored config will not parse must not run under a config this
/// daemon guessed at, and the parser's own error is what says which rule the
/// document broke. Converting it to a string here would destroy that.
impl From<afd_fleet_runtime::Error> for Error {
    fn from(source: afd_fleet_runtime::Error) -> Self {
        Self::new(ErrorKind::ConfigUnreadable { source })
    }
}

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
