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
pub mod lift;
pub mod refuse;

pub use self::detail::{
    DETAIL_BUDGET_EXHAUSTED, DETAIL_BUNDLE_FETCH_FAILED, DETAIL_BUNDLE_NOT_FOUND,
    DETAIL_BUNDLE_STORAGE_UNAVAILABLE, DETAIL_CONFIG_UNREADABLE, DETAIL_CREDENTIAL_MISSING,
    DETAIL_DATABASE_ERROR, DETAIL_DATABASE_UNAVAILABLE, DETAIL_EVENT_MALFORMED,
    DETAIL_GATE_BINDING_UNWRITABLE, DETAIL_GATE_REFERENCE_UNWRITABLE, DETAIL_HOST_ID_BOUNDS,
    DETAIL_LEASE_LOST, DETAIL_LEASE_MAX_RUNTIME, DETAIL_LEASE_NOT_FOUND,
    DETAIL_PROVIDER_UNRESOLVED, DETAIL_QUEUE_UNAVAILABLE, DETAIL_REGISTRATION_FAILED,
    DETAIL_REGISTRY_ALLOWLIST, DETAIL_RENEWAL_NO_CREDITS, DETAIL_RUNNER_NOT_FOUND,
    DETAIL_STALE_FENCE, DETAIL_VAULT_DATA_INVALID,
};
// The mint family's sentences, listed apart from the block above only because
// they arrived together and are read together — `credentials_mint.zig` writes
// all ten, and every one is pinned byte-for-byte to it.
// The device-flow login family's sentences, listed apart for the reason the
// mint family's are: they arrive together, they are read together, and every
// one is pinned to `session_helpers.zig`.
pub use self::detail::{
    DETAIL_APIKEY_ALREADY_REVOKED, DETAIL_APIKEY_DESCRIPTION, DETAIL_APIKEY_MUST_REVOKE_FIRST,
    DETAIL_APIKEY_NAME, DETAIL_APIKEY_NAME_TAKEN, DETAIL_APIKEY_NOT_FOUND,
    DETAIL_APIKEY_READONLY_FIELD,
};
pub use self::detail::{
    DETAIL_BINDING_DRIFT, DETAIL_CONNECTOR_MINT_FAILED, DETAIL_CONNECTOR_RECONNECT,
    DETAIL_GITHUB_RECONNECT, DETAIL_GRANT_REQUIRED, DETAIL_INTEGRATION_NOT_CONNECTED,
    DETAIL_MINT_FAILED, DETAIL_MINT_UNCONFIGURED, DETAIL_WRITE_SPEND_EXHAUSTED,
    DETAIL_WRITE_UNAPPROVED,
};
pub use self::detail::{
    DETAIL_SESSION_ABORTED, DETAIL_SESSION_ALREADY_APPROVED, DETAIL_SESSION_CIPHERTEXT,
    DETAIL_SESSION_CODE_REJECTED, DETAIL_SESSION_CODE_SHAPE, DETAIL_SESSION_CONSUMED,
    DETAIL_SESSION_EXPIRED, DETAIL_SESSION_MISSING, DETAIL_SESSION_NONCE,
    DETAIL_SESSION_NOT_APPROVED, DETAIL_SESSION_NOT_OWNER, DETAIL_SESSION_PUBLIC_KEY,
    DETAIL_SESSION_RATE_LIMITED, DETAIL_SESSION_TOKEN_NAME,
};
pub(crate) use self::refuse::{
    binding_drift, budget_exhausted, connector_mint_failed, connector_reconnect_required,
    github_mint_failed, github_reconnect_required, grant_required, integration_not_connected,
    lease_lost, lease_max_runtime, lease_not_found, mint_unconfigured, renewal_no_credits,
    stale_fence, write_spend_exhausted, write_unapproved,
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

    #[error("a stored fencing sequence is not a sequence")]
    SequenceCorrupt,

    #[error("the reporting holder's fence is behind the fleet's live sequence")]
    StaleFence,

    #[error("no lease with that id belongs to the presenting runner")]
    LeaseNotFound,

    #[error("the lease is no longer this runner's to renew")]
    LeaseLost,

    #[error("the lease reached the hard ceiling on one run's wall time")]
    LeaseMaxRuntime,

    #[error("the tenant's credit pool cannot fund another slice of this run")]
    RenewalNoCredits,

    #[error("the fleet reached a spend ceiling its own author declared")]
    BudgetExhausted,

    #[error("no Fleet Bundle snapshot is stored under that content hash")]
    BundleMissing,

    #[error("this deployment has no Fleet Bundle snapshot storage configured")]
    BundleUnconfigured,

    #[error("the Fleet Bundle snapshot store would not answer")]
    BundleStorage {
        #[source]
        source: object_store::Error,
    },

    #[error("a stored Fleet Bundle snapshot is larger than this daemon buffers: {size} bytes")]
    BundleOversized { size: u64 },

    #[error("this workspace has connected no integration under that name")]
    IntegrationNotConnected,

    #[error("this deployment holds no platform credential for that connector")]
    MintUnconfigured,

    #[error("the GitHub App installation this handle names is gone")]
    GithubReconnectRequired,

    #[error("GitHub returned no installation token this daemon would hand over")]
    GithubMintFailed,

    #[error("the connector's stored authorization is no longer redeemable")]
    ConnectorReconnectRequired,

    #[error("the connector's token exchange produced no credential")]
    ConnectorMintFailed,

    #[error("the fleet holds no approved grant for that integration")]
    GrantRequired,

    #[error("no approved repository-write gate was answered for this lease's event")]
    WriteUnapproved,

    #[error("the fleet's repository binding changed since the approval was answered")]
    BindingDrift,

    #[error("the approved write-credential allowance is spent")]
    WriteSpendExhausted,

    #[error("a device-flow login field was refused: {field}")]
    SessionFieldInvalid { field: SessionField },

    #[error("no device-flow login session is held under that id")]
    SessionMissing,

    #[error("the device-flow login session's window closed before it was redeemed")]
    SessionExpired,

    #[error("the device-flow login session was already redeemed")]
    SessionConsumed,

    #[error("the device-flow login session was cancelled, superseded, or rate-limited")]
    SessionAborted,

    #[error("the device-flow login session was aborted by its own attempt ceiling")]
    SessionRateLimited,

    #[error("no human has approved this device-flow login session yet")]
    SessionNotApproved,

    #[error("this device-flow login session is already past pending")]
    SessionAlreadyApproved,

    #[error("the presented code did not match the session's stored digest")]
    SessionCodeRejected,

    #[error("this device-flow login session belongs to another identity")]
    SessionNotOwner,

    #[error("an api-key field was refused: {field}")]
    ApiKeyFieldInvalid { field: ApiKeyField },

    #[error("no api-key with that id belongs to this tenant")]
    ApiKeyNotFound,

    #[error("this tenant already holds an api-key under that name")]
    ApiKeyNameTaken,

    #[error("this api-key was already revoked, so nothing changed")]
    ApiKeyAlreadyRevoked,

    #[error("an api-key cannot be brought back once revoked")]
    ApiKeyReadonlyField,

    #[error("an active api-key must be revoked before it can be deleted")]
    ApiKeyMustRevokeFirst,
}

/// Which api-key field a refusal names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiKeyField {
    /// The name the key is listed and grepped under.
    Name,
    /// The free text beside it.
    Description,
}

impl Display for ApiKeyField {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Name => "key_name",
            Self::Description => "description",
        })
    }
}

/// Which device-flow field a refusal names.
///
/// One kind with a field rather than five kinds, because the five differ in
/// exactly one way — the code and sentence they answer with — and a table
/// keyed on the field says that once. The Zig store spells five error tags and
/// `failFromStoreError` re-pairs each with its code and its sentence at a
/// switch arm, which is the same fact written three times.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionField {
    /// The command line's public key, presented at creation.
    PublicKey,
    /// The label the minted credential will carry.
    TokenName,
    /// The relayed envelope the dashboard sealed.
    Ciphertext,
    /// The nonce that envelope was sealed under.
    Nonce,
    /// The six digits a person reads out of the browser.
    VerificationCode,
}

impl Display for SessionField {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::PublicKey => "public_key",
            Self::TokenName => "token_name",
            Self::Ciphertext => "ciphertext",
            Self::Nonce => "nonce",
            Self::VerificationCode => "verification_code",
        })
    }
}

/// The refusals a suite outside this crate needs to CONSTRUCT.
///
/// M-TEST-UTIL. The router suites stub the lease plane, and a stub for the mint
/// verb has no honest success to answer with — it holds no vault row, no grant
/// and no vendor — so it answers the refusal a deployment with no platform
/// credentials gives. Every other constructor stays `pub(crate)`: this is a
/// seam for a stub, not a way for another crate to invent this plane's
/// refusals.
#[cfg(feature = "test-util")]
impl Error {
    /// The refusal a deployment holding no platform credential answers.
    #[must_use]
    pub fn mint_unconfigured() -> Self {
        super::error::mint_unconfigured()
    }

    /// The refusal a verb answers when the datastore behind it would not answer.
    ///
    /// Exposed for the reason [`Error::queue_unavailable`] is: a router suite
    /// stubs services whose whole behaviour lives in statements a real Postgres
    /// evaluates, and the honest stub answers the one refusal that is true of
    /// it rather than inventing a success.
    #[must_use]
    pub fn datastore_unavailable() -> Self {
        Self::new(ErrorKind::Datastore {
            source: afd_db::error::unavailable_for_test(),
        })
    }

    /// The refusal a verb answers when the queue behind it would not answer.
    ///
    /// Exposed for the same reason [`Error::mint_unconfigured`] is: a router
    /// suite stubs the device-flow surface, and every one of that surface's
    /// verbs lives inside a Lua script a real Redis evaluates. There is no
    /// success a stub could invent that would not be inventing the state
    /// machine, so it answers the one refusal that is true of it.
    #[must_use]
    pub fn queue_unavailable() -> Self {
        Self::new(ErrorKind::Queue {
            source: afd_redis::error::unavailable_for_test(),
        })
    }
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

/// Reports a stored fencing sequence that cannot be one.
///
/// Its own kind rather than a saturating read, and the direction is why.
/// [`Fence::as_u64`](crate::lease::Fence::as_u64) saturates a negative token to
/// ZERO because zero is below every token a claim can mint, so a corrupt row
/// fences ITSELF out. The live sequence a memory push is checked against runs
/// the other way: saturating it to zero would put it below every token in
/// existence and admit every stale holder. There is no safe value, so there is
/// no value.
pub(crate) fn sequence_corrupt() -> Error {
    Error::new(ErrorKind::SequenceCorrupt)
}

/// Reports a content hash with no snapshot stored under it.
///
/// The ordinary answer for a skill-only bundle, not a fault — see
/// [`crate::bundle::Bundles::fetch`].
pub(crate) fn bundle_missing() -> Error {
    Error::new(ErrorKind::BundleMissing)
}

/// Reports a deployment that never configured snapshot storage.
pub(crate) fn bundle_unconfigured() -> Error {
    Error::new(ErrorKind::BundleUnconfigured)
}

/// Reports an object store that was reached and would not serve.
///
/// The store's own error rides through as `#[source]` rather than being
/// stringified into a message: a refused signature, an unresolvable endpoint
/// and a missing bucket are three different operator problems, and the chain is
/// the only place that distinction survives (`RUST_ERROR_STANDARD` rule 3).
pub(crate) fn bundle_storage(source: object_store::Error) -> Error {
    Error::new(ErrorKind::BundleStorage { source })
}

/// Reports a stored object too large for this daemon to buffer.
///
/// Its own kind rather than a storage failure, because it is not one: the store
/// answered correctly and what it holds is the problem. The size is carried so
/// the operator's log line names it — nothing puts it on the wire.
pub(crate) fn bundle_oversized(size: u64) -> Error {
    Error::new(ErrorKind::BundleOversized { size })
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

/// Refuses a device-flow field this daemon will not store.
pub(crate) fn session_field(field: SessionField) -> Error {
    Error::new(ErrorKind::SessionFieldInvalid { field })
}

/// Reports a session id naming nothing this daemon holds.
pub(crate) fn session_missing() -> Error {
    Error::new(ErrorKind::SessionMissing)
}

/// Reports a session whose five-minute window closed.
pub(crate) fn session_expired() -> Error {
    Error::new(ErrorKind::SessionExpired)
}

/// Reports a session already redeemed.
pub(crate) fn session_consumed() -> Error {
    Error::new(ErrorKind::SessionConsumed)
}

/// Reports a session cancelled, superseded, or rate-limited before this call.
pub(crate) fn session_aborted() -> Error {
    Error::new(ErrorKind::SessionAborted)
}

/// Reports the wrong attempt that just tripped the session's own ceiling.
pub(crate) fn session_rate_limited() -> Error {
    Error::new(ErrorKind::SessionRateLimited)
}

/// Reports a code presented before any human approved the session.
pub(crate) fn session_not_approved() -> Error {
    Error::new(ErrorKind::SessionNotApproved)
}

/// Reports a second approval of one session.
pub(crate) fn session_already_approved() -> Error {
    Error::new(ErrorKind::SessionAlreadyApproved)
}

/// Reports six digits that did not match the stored digest.
pub(crate) fn session_code_rejected() -> Error {
    Error::new(ErrorKind::SessionCodeRejected)
}

/// Reports an abort attempted by an identity that does not hold the session.
pub(crate) fn session_not_owner() -> Error {
    Error::new(ErrorKind::SessionNotOwner)
}

/// Refuses an api-key field this daemon will not store.
pub(crate) fn apikey_field(field: ApiKeyField) -> Error {
    Error::new(ErrorKind::ApiKeyFieldInvalid { field })
}

/// Reports an id naming no key this tenant holds.
pub(crate) fn apikey_not_found() -> Error {
    Error::new(ErrorKind::ApiKeyNotFound)
}

/// Reports a name this tenant already uses.
pub(crate) fn apikey_name_taken() -> Error {
    Error::new(ErrorKind::ApiKeyNameTaken)
}

/// Reports a revoke of a key that was already revoked.
pub(crate) fn apikey_already_revoked() -> Error {
    Error::new(ErrorKind::ApiKeyAlreadyRevoked)
}

/// Reports an attempt to re-activate a revoked key.
pub(crate) fn apikey_readonly_field() -> Error {
    Error::new(ErrorKind::ApiKeyReadonlyField)
}

/// Reports a delete of a key that is still active.
pub(crate) fn apikey_must_revoke_first() -> Error {
    Error::new(ErrorKind::ApiKeyMustRevokeFirst)
}
