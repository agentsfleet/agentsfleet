//! Why a request was not sent, or brought nothing back.
//!
//! The vocabulary every egress tool answers the model with, kept a plain enum
//! the way the tool error codes are: it is read, never matched by a caller
//! that would retry, and carries no cause. A refusal names a host, a method or
//! a credential's name, and never a secret's value, a header's value or a
//! body, because the model and the thread both read it.

/// One request that did not leave, or left and failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
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
    #[error("no request rule at {host} admits {method} {path} with this body")]
    RequestPolicyNotAllowed {
        /// The URL's host.
        host: String,
        /// The method.
        method: String,
        /// The URL's path.
        path: String,
    },

    /// The daemon would not mint the credential.
    #[error("the credential could not be minted: {detail}")]
    CredentialMintRefused {
        /// The daemon's own words.
        detail: String,
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

impl Refusal {
    /// The fleet has no secret `name.field`.
    #[must_use]
    pub fn secret_not_found(name: &str, field: &str) -> Self {
        Self::SecretNotFound {
            name: name.to_owned(),
            field: field.to_owned(),
        }
    }
}
