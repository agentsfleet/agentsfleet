//! What this crate refuses, and what it reports.
//!
//! The hull — the boxed struct, the backtrace, `Display`, `source()` — comes
//! from [`afd_core::error_shell!`]. What is here is the part that varies: the
//! kinds this plane can fail with, the code and sentence each answers, and the
//! raisers that bind data.

use afd_core::error_code::{self, ErrorCode};

#[cfg(test)]
mod tests;

/// The result every fallible function in this crate returns.
///
/// One alias per crate, defaulted to this crate's own [`Error`], so a reader
/// never has to check WHICH error a signature returns to know it is this one
/// (`RUST_ERROR_STANDARD` rule 1).
pub type Result<T, E = Error> = core::result::Result<T, E>;

afd_core::error_shell!(
    /// A credential-plane failure, with the backtrace of where it was raised.
    pub struct Error(ErrorKind);
);

/// What actually went wrong. Crate-visible so a raise site can name the variant.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    #[error("the datastore backing the credential plane would not answer")]
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

    #[error("a {table} row holds a value this daemon cannot read: {column}")]
    RowMalformed {
        table: &'static str,
        column: &'static str,
        #[source]
        source: afd_core::error::Error,
    },

    #[error("a stored provider selection is malformed: {field}")]
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

    #[error("could not draw the entropy a registry entry is minted from")]
    Entropy {
        #[source]
        source: afd_crypto::error::Error,
    },

    #[error("a minted registry-entry identifier was not well-formed")]
    Mint {
        #[source]
        source: afd_core::error::Error,
    },
}

impl Error {
    /// The code and the sentence, decided together.
    ///
    /// One table rather than two matches: a code and the sentence it is served
    /// with are one decision, and splitting them is how a variant acquires a
    /// code from one arm and a sentence from another.
    const fn answer(&self) -> (ErrorCode, &'static str) {
        match self.kind() {
            ErrorKind::Datastore { .. } => {
                (error_code::INTERNAL_DB_UNAVAILABLE, DETAIL_UNAVAILABLE)
            }
            ErrorKind::Query { .. } | ErrorKind::RowMalformed { .. } => {
                (error_code::INTERNAL_DB_QUERY, DETAIL_DATABASE_ERROR)
            }
            // The provider family answers one code, matching what
            // `service_billing.zig` logs for the whole of it. The finer
            // `UZ-PROVIDER-*` codes belong to the tenant plane's handler;
            // declaring them on a path that cannot emit them would be an
            // unreferenced code that looks like coverage.
            //
            // `Vault` joins them because that is what `crypto_store.zig` logs
            // when an envelope will not open.
            ErrorKind::ProviderMalformed { .. }
            | ErrorKind::ProviderSecretMissing
            | ErrorKind::ProviderPlatformKeyMissing
            | ErrorKind::ProviderNoWorkspace
            | ErrorKind::ProviderEndpoint { .. }
            | ErrorKind::Vault { .. } => (
                error_code::INTERNAL_OPERATION_FAILED,
                DETAIL_PROVIDER_UNRESOLVED,
            ),
            // Two vault failures, two codes, matching the two the Zig logs:
            // an envelope that will not open answers the internal code above
            // because WHICH check failed is an oracle, while a body whose
            // shape is wrong answers this one — the shape is a fact the
            // operator who stored it can act on.
            ErrorKind::VaultDataInvalid => (error_code::VAULT_DATA_INVALID, DETAIL_VAULT_INVALID),
            // Two failures of this instance rather than of its input: a host
            // that cannot draw entropy, and a mint that produced something
            // `Uuid7` refuses. Neither is the caller's to correct, and both
            // answer the same internal code `afd_tenant` gives them.
            ErrorKind::Entropy { .. } | ErrorKind::Mint { .. } => {
                (error_code::INTERNAL_OPERATION_FAILED, DETAIL_DATABASE_ERROR)
            }
            // The one provider-family failure an operator can ACT on: the
            // fleet named a credential and nobody stored it.
            ErrorKind::CredentialMissing => (
                error_code::AGENTSFLEET_CREDENTIAL_MISSING,
                DETAIL_CREDENTIAL_MISSING,
            ),
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
    /// problem and answers 503, where everything else here is a 500.
    #[must_use]
    pub const fn is_datastore_unavailable(&self) -> bool {
        matches!(self.kind(), ErrorKind::Datastore { .. })
    }

    /// Whether this failure is a stored-configuration fault a human must fix.
    ///
    /// The question the lease's block path turns on: a permanent fault earns a
    /// terminal row that puts the fleet in front of a person, where a transient
    /// one leaves the delivery leasable for the next poll.
    ///
    /// A stored endpoint the SSRF guard refused, a selection naming a vault row
    /// nobody holds, a body that is not an addressable object — none of those
    /// becomes correct by being read again. An envelope that will not OPEN is
    /// deliberately not among them: it is usually permanent too, but it is also
    /// what a truncated read or a half-written row looks like, and those
    /// recover. A stored URL is data this daemon parsed and rejected; an
    /// unopened envelope is data it never got to see.
    #[must_use]
    pub const fn is_config_permanent(&self) -> bool {
        matches!(
            self.kind(),
            ErrorKind::ProviderMalformed { .. }
                | ErrorKind::ProviderSecretMissing
                | ErrorKind::ProviderPlatformKeyMissing
                | ErrorKind::ProviderNoWorkspace
                | ErrorKind::ProviderEndpoint { .. }
                | ErrorKind::VaultDataInvalid
                | ErrorKind::CredentialMissing
        )
    }

    /// The guard's word for a refused endpoint, when that is what failed.
    ///
    /// Activation renders this refusal as an outcome a client is told, where
    /// every other provider-family failure is an internal fault. The word is
    /// the guard's own — never the URL and never the host, which sit beside an
    /// `api_key` in the same credential.
    #[must_use]
    pub const fn endpoint_rejection(&self) -> Option<&'static str> {
        match self.kind() {
            ErrorKind::ProviderEndpoint { reason } => Some(reason),
            _not_an_endpoint => None,
        }
    }

    /// Whether a declared credential simply has no vault row.
    ///
    /// Answered separately because the remedy is the operator's and not this
    /// daemon's: store the secret the fleet named.
    #[must_use]
    pub const fn is_credential_missing(&self) -> bool {
        matches!(self.kind(), ErrorKind::CredentialMissing)
    }
}

/// The sentences this plane serves.
const DETAIL_UNAVAILABLE: &str = "Database unavailable";
const DETAIL_DATABASE_ERROR: &str = "The operation could not be completed";
const DETAIL_PROVIDER_UNRESOLVED: &str = "The model provider could not be resolved";
const DETAIL_VAULT_INVALID: &str = "The stored credential is not a readable shape";
const DETAIL_CREDENTIAL_MISSING: &str =
    "The fleet declared a credential this workspace does not hold";

afd_core::error_lifts!(Error, ErrorKind:
    afd_db::Error => Datastore,
);

/// Reports a statement that would not run, naming what it was doing.
pub(crate) fn query(context: &'static str) -> impl Fn(sqlx::Error) -> Error {
    move |source| ErrorKind::Query { context, source }.into()
}

/// Reports a stored value this build cannot read, naming table and column.
pub(crate) fn row_malformed(
    table: &'static str,
    column: &'static str,
) -> impl Fn(afd_core::error::Error) -> Error {
    move |source| {
        ErrorKind::RowMalformed {
            table,
            column,
            source,
        }
        .into()
    }
}

/// Reports a stored provider selection this daemon cannot read.
pub(crate) fn provider_malformed(field: &'static str) -> Error {
    ErrorKind::ProviderMalformed { field }.into()
}

/// Reports a provider selection naming a vault row that is not held.
pub(crate) fn provider_secret_missing() -> Error {
    ErrorKind::ProviderSecretMissing.into()
}

/// Reports that no operator has set an active platform default.
pub(crate) fn provider_platform_key_missing() -> Error {
    ErrorKind::ProviderPlatformKeyMissing.into()
}

/// Reports a tenant with no workspace to resolve a credential in.
pub(crate) fn provider_no_workspace() -> Error {
    ErrorKind::ProviderNoWorkspace.into()
}

/// Reports a stored endpoint the SSRF guard refused.
///
/// `reason` is the guard's own word for what it refused, never the URL and
/// never the host — the `api_key` sits beside both in the same credential, and
/// a rejection line that quotes the credential is one that leaks it.
pub(crate) fn provider_endpoint(reason: &'static str) -> Error {
    ErrorKind::ProviderEndpoint { reason }.into()
}

/// Reports a stored envelope that is malformed or will not authenticate.
///
/// A conversion that ADDS the context the call site alone knows, with the cause
/// riding through as `#[source]` so the chain stays intact — the carve-out
/// `RUST_ERROR_STANDARD` rule 3 names.
pub(crate) fn vault_open(source: afd_crypto::error::Error) -> Error {
    ErrorKind::Vault { source }.into()
}

/// Reports a stored credential body that is not an addressable object.
pub(crate) fn vault_data_invalid() -> Error {
    ErrorKind::VaultDataInvalid.into()
}

/// Reports a host that could not draw entropy for a minted identifier.
///
/// A `map_err` that ADDS what the call site alone knows — WHICH mint drained —
/// with the cause riding through as `#[source]`, per `RUST_ERROR_STANDARD`
/// rule 3. Not an `error_lifts!` entry: `afd_crypto::error::Error` already
/// means "an envelope would not open" on this crate's vault path, and one
/// `From` cannot mean both.
pub(crate) fn entropy_drained(source: afd_crypto::error::Error) -> Error {
    ErrorKind::Entropy { source }.into()
}

/// Reports a minted identifier the domain type refused.
pub(crate) fn mint_failed(source: afd_core::error::Error) -> Error {
    ErrorKind::Mint { source }.into()
}

/// Reports a declared credential with no vault row.
///
/// The log line — with the workspace and the name an operator needs — is
/// written at the call site, which is the only place that holds them.
pub(crate) fn credential_missing() -> Error {
    ErrorKind::CredentialMissing.into()
}
