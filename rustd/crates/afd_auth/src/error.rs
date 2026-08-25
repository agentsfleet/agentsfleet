//! Why a credential was refused, with its registry code attached.
//!
//! The Zig daemon spells a refusal as a pair of arguments at a call site —
//! `ctx.fail(errors.ERR_UNAUTHORIZED, S_INVALID_OR_MISSING_TOKEN)` — twelve
//! times across four middleware files. Nothing relates the two arguments, so
//! the pairing is a convention each site restates, and the detail strings are
//! per-file constants that happen to agree.
//!
//! Here the code and the detail are PROPERTIES OF THE REFUSAL. A caller
//! constructs the reason; it cannot construct a reason paired with the wrong
//! code, because there is no place to write the code down. [`AuthError::code`]
//! is the whole mapping, in one exhaustive match, which is also the only place
//! a reviewer has to look to check it.
//!
//! # The detail strings are client-visible, so they are pinned
//!
//! Parity in this milestone is behavioural, and what a client reads is
//! behaviour. Every string below is byte-identical to the Zig constant it
//! replaces, and `test_detail_matches_zig` reads the Zig sources and fails if
//! either side moves.
//!
//! # Why so many refusals collapse onto one code
//!
//! A missing header, a malformed one, a well-formed credential no row matches,
//! and a signature that does not verify all answer `UZ-AUTH-002` with the same
//! sentence. That is not laziness: an unauthenticated caller learning WHICH of
//! their guesses was closer is the one thing this surface must not teach. The
//! distinctions worth keeping are the ones that are either useless to an
//! attacker (an expired token was already validly signed) or actionable by a
//! legitimate holder (this credential is revoked, stop retrying it).

use afd_core::error_code::{self, ErrorCode};

/// The result every fallible function in this crate returns.
///
/// One alias per crate, defaulted to that crate's own [`AuthError`] — the shape
/// `core_api` has run in production on for years, and the one bun uses
/// (`pub type Result<T, E = Error>`). The default parameter is what lets the
/// few functions answering with a different error keep the same spelling:
/// `Result<T>` for the common case, `Result<T, OtherError>` where it differs.
///
/// The point is not brevity. It is that a reader never has to check WHICH
/// error a signature returns to know it is this crate's, and a new call site
/// cannot quietly introduce a second error type without saying so.
pub type Result<T, E = AuthError> = core::result::Result<T, E>;

/// The Zig `S_INVALID_OR_MISSING_TOKEN`, shared by the three tenant-plane
/// classes. One spelling, because three copies is three chances to drift and
/// the difference would be visible to a client (RULE UFS).
const S_INVALID_OR_MISSING_TOKEN: &str = "Invalid or missing token";
/// The Zig `S_AUTH_UNAVAILABLE`.
const S_AUTH_UNAVAILABLE: &str = "Authentication service unavailable";
/// `bearer_or_api_key.zig`'s expiry detail, lower-cased exactly as it is there.
const S_TOKEN_EXPIRED: &str = "token expired";
/// `tenant_api_key.zig`'s revocation detail.
const S_APIKEY_REVOKED: &str = "API key has been revoked";
/// `cli_credential.zig`'s `S_REVOKED_MESSAGE`.
const S_CLI_CREDENTIAL_REVOKED: &str = "Command-line credential has been revoked";
/// `runner_bearer.zig`'s `S_INVALID_OR_MISSING_TOKEN` — a DIFFERENT sentence
/// from the tenant plane's, naming the runner token, and kept distinct because
/// the runner client reads it.
const S_INVALID_RUNNER_TOKEN: &str = "Invalid or missing runner token";
/// `runner_bearer.zig`'s `S_RUNNER_ADMIN_STATE_BLOCKED`.
const S_RUNNER_STATE_BLOCKED: &str = "Runner admin state blocks runner-plane access";

/// A credential was not accepted, and why.
///
/// Carries no detail from the credential itself — not the prefix, not a
/// fragment, not the digest. A refusal that quotes what it refused is a refusal
/// that ends up in a log beside the value it was quoting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum AuthError {
    /// Nothing was presented, or what was presented proved nothing.
    #[error("{S_INVALID_OR_MISSING_TOKEN}")]
    InvalidOrMissingToken,

    /// The token verified and its expiry has passed.
    #[error("{S_TOKEN_EXPIRED}")]
    TokenExpired,

    /// What judges the credential could not be reached.
    ///
    /// Never a rejection. See [`error_code::AUTH_UNAVAILABLE`] for why the
    /// runner plane in particular depends on this staying distinct.
    #[error("{S_AUTH_UNAVAILABLE}")]
    Unavailable,

    /// The tenant api-key resolved to a row that is no longer active.
    #[error("{S_APIKEY_REVOKED}")]
    TenantKeyRevoked,

    /// The command-line credential resolved to a row with `revoked_at` set.
    #[error("{S_CLI_CREDENTIAL_REVOKED}")]
    CliCredentialRevoked,

    /// The runner plane's [`AuthError::InvalidOrMissingToken`].
    #[error("{S_INVALID_RUNNER_TOKEN}")]
    InvalidRunnerToken,

    /// The runner is known and its administrative state bars the plane.
    #[error("{S_RUNNER_STATE_BLOCKED}")]
    RunnerStateBlocked,
}

impl AuthError {
    /// The registry code this refusal answers with.
    ///
    /// Exhaustive, so a new variant fails to compile until it is given one —
    /// the same device [`crate::scope::Scope::wire`] uses, applied to the
    /// pairing the Zig call sites restate by hand.
    #[must_use]
    pub const fn code(self) -> ErrorCode {
        match self {
            Self::InvalidOrMissingToken => error_code::AUTH_UNAUTHORIZED,
            Self::TokenExpired => error_code::AUTH_TOKEN_EXPIRED,
            Self::Unavailable => error_code::AUTH_UNAVAILABLE,
            Self::TenantKeyRevoked => error_code::APIKEY_REVOKED,
            Self::CliCredentialRevoked => error_code::AUTH_CLI_CREDENTIAL_REVOKED,
            Self::InvalidRunnerToken => error_code::RUN_INVALID_RUNNER_TOKEN,
            Self::RunnerStateBlocked => error_code::RUN_ADMIN_STATE_BLOCKED,
        }
    }

    /// The sentence a client reads, pinned byte-for-byte against the Zig
    /// daemon's constant.
    ///
    /// Equal to `to_string()` and present so a test can assert the pinning
    /// without allocating, and so the pinning is a named property rather than
    /// an incidental consequence of the `Display` derive.
    #[must_use]
    pub const fn detail(self) -> &'static str {
        match self {
            Self::InvalidOrMissingToken => S_INVALID_OR_MISSING_TOKEN,
            Self::TokenExpired => S_TOKEN_EXPIRED,
            Self::Unavailable => S_AUTH_UNAVAILABLE,
            Self::TenantKeyRevoked => S_APIKEY_REVOKED,
            Self::CliCredentialRevoked => S_CLI_CREDENTIAL_REVOKED,
            Self::InvalidRunnerToken => S_INVALID_RUNNER_TOKEN,
            Self::RunnerStateBlocked => S_RUNNER_STATE_BLOCKED,
        }
    }

    /// Whether this refusal says the credential is bad, as opposed to saying
    /// the daemon could not tell.
    ///
    /// The runner client counts consecutive REJECTIONS toward a
    /// self-termination ceiling and resets that counter on anything else, so
    /// the distinction has to be readable from the error rather than inferred
    /// from its code at each call site.
    #[must_use]
    pub const fn is_rejection(self) -> bool {
        !matches!(self, Self::Unavailable)
    }

    /// Every variant, for the exhaustive walks the tests do.
    ///
    /// Totality is proven by `test_every_variant_is_listed`, which counts this
    /// against the distinct codes and details it produces.
    pub const ALL: [Self; 7] = [
        Self::InvalidOrMissingToken,
        Self::TokenExpired,
        Self::Unavailable,
        Self::TenantKeyRevoked,
        Self::CliCredentialRevoked,
        Self::InvalidRunnerToken,
        Self::RunnerStateBlocked,
    ];
}

/// A dependency this crate does not own could not be reached.
///
/// The one error both the [`crate::directory::CredentialDirectory`] and
/// [`crate::capability::CapabilitySource`] seams may return, and it is
/// deliberately opaque: an implementation's reason — a pool timeout, a 502
/// from the provider, a DNS failure — is an operator's concern, logged where it
/// happens, and never a fact the authentication decision branches on. Every one
/// of them means the same thing here, so they arrive as the same type and
/// become [`AuthError::Unavailable`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{S_AUTH_UNAVAILABLE}")]
pub struct Unavailable;

impl From<Unavailable> for AuthError {
    fn from(_unavailable: Unavailable) -> Self {
        Self::Unavailable
    }
}
