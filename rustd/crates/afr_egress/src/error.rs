//! What the egress guard fails with.
//!
//! One error type with `pub type Result<T, E = Error>` beside it, under the
//! `afd_core::error_shell!` hull: a request the guard refused, or sent and got
//! nothing back for, carries its host, method or credential name, so it takes
//! the hull (`docs/RUST_ERROR_STANDARD.md` §"The shared hull"). Every refusal
//! goes back to the model as an error: its [`Error::detail`] is the sentence
//! the model reads, never the `[CODE]` rendering, and [`Error::refusal`] is
//! what a tool picks the model's code from. A refusal names a host, a method or
//! a credential's name, and never a secret's value, a header's value or a
//! body, because the model and the thread both read it.
//!
//! # Which codes
//!
//! Existing registry codes, as the runner's other crates reuse them: a request
//! the guard would not send is the fleet's request refused, a secret it lacks
//! is the vault's, a mint the daemon refused keeps the daemon's own code, and
//! an upstream that never answered or a client that could not be built is an
//! internal failure.

use afd_core::error_code::{self, ErrorCode};

use crate::refusal::Refusal;

pub(crate) mod raise;

afd_core::error_shell!(
    /// An egress failure, with the backtrace of where it was raised.
    pub struct Error(ErrorKind);
);

/// Every way the guard refuses a request, a send fails, or the client is not
/// built.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    /// reqwest refused the client's configuration.
    #[error("the egress client could not be built")]
    Client {
        /// reqwest's refusal.
        #[from]
        source: reqwest::Error,
    },

    /// The URL does not parse.
    #[error("the URL does not parse: {reason}")]
    InvalidUrl {
        /// What the parser objected to.
        reason: url::ParseError,
    },

    /// A header's name or value is not one HTTP can carry.
    #[error("header {name} cannot be sent")]
    InvalidHeader {
        /// The header's name, as the model wrote it.
        name: String,
    },

    /// The URL is not HTTPS.
    #[error("only https URLs are sent")]
    HttpsRequired,

    /// The method is outside what this tool or this policy sends.
    #[error("{method} is not sent here")]
    MethodNotAllowed {
        /// The method, as the model wrote it.
        method: String,
    },

    /// The host is not in the fleet's network allowlist.
    #[error("{host} is not in this fleet's network allowlist")]
    HostNotAllowed {
        /// The URL's host.
        host: String,
    },

    /// The host is, or resolves to, an address this runner never reaches.
    #[error("{host} is a private, loopback or reserved address")]
    AddressNotAllowed {
        /// The URL's host.
        host: String,
    },

    /// A placeholder, or a header, stands where none may.
    #[error(
        "a secret placeholder may stand only as the Authorization header's value or as \
         the URL's whole host, and {what} may not be set"
    )]
    PlacementNotAllowed {
        /// What stood where it may not.
        what: String,
    },

    /// A credential named by a placeholder may not be sent to this host.
    #[error("credential {name} is not sent to {host}")]
    CredentialHostNotAllowed {
        /// The credential's name.
        name: String,
        /// The URL's host.
        host: String,
    },

    /// A placeholder names a secret this fleet does not have.
    #[error("this fleet has no secret {name}.{field}")]
    SecretNotFound {
        /// The credential's name.
        name: String,
        /// The field asked for.
        field: String,
    },

    /// The host's origin rules admit no request of this shape.
    #[error(
        "no request rule at {host} admits {method} {path} with this body{}",
        .why.as_deref().map(|why| format!(": {why}")).unwrap_or_default()
    )]
    RequestPolicyNotAllowed {
        /// The URL's host.
        host: String,
        /// The method.
        method: String,
        /// The URL's path.
        path: String,
        /// What a closed rule at this method and path refused, when one did.
        why: Option<String>,
    },

    /// The daemon would not mint the credential.
    #[error("the credential could not be minted: {detail}")]
    CredentialMintRefused {
        /// The daemon's registry code for the refusal.
        code: ErrorCode,
        /// The daemon's own words.
        detail: String,
    },

    /// A minted token could not join the masker, so it was never used.
    #[error(
        "the credential could not be minted: the minted token could not be masked, so it \
         was not used"
    )]
    Unmaskable {
        /// The scrub's refusal.
        #[source]
        source: afr_secrets::Error,
    },

    /// The request left and no answer came back.
    #[error("{host} could not be reached: {reason}")]
    UpstreamUnreachable {
        /// The URL's host.
        host: String,
        /// What failed, in a fixed phrase that carries no URL.
        reason: &'static str,
    },
}

/// The one alias every signature in this crate spells.
pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    /// The fleet has no secret `name.field`.
    #[must_use]
    pub fn secret_not_found(name: &str, field: &str) -> Self {
        Self::from(ErrorKind::SecretNotFound {
            name: name.to_owned(),
            field: field.to_owned(),
        })
    }

    /// The daemon would not mint a credential: `detail` in the words the
    /// model reads, which must carry no secret, under the daemon's `code`.
    #[must_use]
    pub fn mint_refused(code: ErrorCode, detail: impl Into<String>) -> Self {
        Self::from(ErrorKind::CredentialMintRefused {
            code,
            detail: detail.into(),
        })
    }

    /// The failure in the sentence the model reads, without its code or
    /// backtrace.
    #[must_use]
    pub fn detail(&self) -> String {
        self.kind().to_string()
    }

    /// The refusal a tool tells the model this was; none for a client that
    /// could not be built, which no request ever meets.
    #[must_use]
    pub const fn refusal(&self) -> Option<Refusal> {
        let refusal = match self.kind() {
            ErrorKind::Client { .. } => return None,
            ErrorKind::InvalidUrl { .. } => Refusal::InvalidUrl,
            ErrorKind::InvalidHeader { .. } => Refusal::InvalidHeader,
            ErrorKind::HttpsRequired => Refusal::HttpsRequired,
            ErrorKind::MethodNotAllowed { .. } => Refusal::MethodNotAllowed,
            ErrorKind::HostNotAllowed { .. } => Refusal::HostNotAllowed,
            ErrorKind::AddressNotAllowed { .. } => Refusal::AddressNotAllowed,
            ErrorKind::PlacementNotAllowed { .. } => Refusal::PlacementNotAllowed,
            ErrorKind::CredentialHostNotAllowed { .. } => Refusal::CredentialHostNotAllowed,
            ErrorKind::SecretNotFound { .. } => Refusal::SecretNotFound,
            ErrorKind::RequestPolicyNotAllowed { .. } => Refusal::RequestPolicyNotAllowed,
            ErrorKind::CredentialMintRefused { .. } | ErrorKind::Unmaskable { .. } => {
                Refusal::CredentialMintRefused
            }
            ErrorKind::UpstreamUnreachable { .. } => Refusal::UpstreamUnreachable,
        };
        Some(refusal)
    }

    /// The registry code this failure is logged under.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self.kind() {
            ErrorKind::InvalidUrl { .. }
            | ErrorKind::InvalidHeader { .. }
            | ErrorKind::HttpsRequired
            | ErrorKind::MethodNotAllowed { .. }
            | ErrorKind::HostNotAllowed { .. }
            | ErrorKind::AddressNotAllowed { .. }
            | ErrorKind::PlacementNotAllowed { .. }
            | ErrorKind::CredentialHostNotAllowed { .. }
            | ErrorKind::RequestPolicyNotAllowed { .. } => error_code::INVALID_REQUEST,
            ErrorKind::SecretNotFound { .. } => error_code::SECRET_NOT_FOUND,
            ErrorKind::CredentialMintRefused { code, .. } => *code,
            ErrorKind::Client { .. }
            | ErrorKind::Unmaskable { .. }
            | ErrorKind::UpstreamUnreachable { .. } => error_code::INTERNAL_OPERATION_FAILED,
        }
    }
}

#[cfg(any(test, feature = "test-util"))]
pub use self::raise::one_of_each_kind;

#[cfg(test)]
#[path = "error/tests.rs"]
mod tests;
