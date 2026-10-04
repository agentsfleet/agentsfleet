//! Which refusal a request met, as a tool names it to the model.
//!
//! A fieldless projection of [`Error`](crate::Error), read through
//! [`Error::refusal`](crate::Error::refusal): the error carries the host or
//! name the model reads in its detail, and a tool discriminates on this to
//! pick the code the model reads first (`afr_tools/src/egress.rs`). Fieldless,
//! so it stays a plain `Copy` enum (`docs/RUST_ERROR_STANDARD.md` §"The shared
//! hull").

/// One request that did not leave, or left and failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The URL does not parse.
    InvalidUrl,
    /// A header's name or value is not one HTTP can carry.
    InvalidHeader,
    /// The URL is not HTTPS.
    HttpsRequired,
    /// The method is outside what this tool or this policy sends.
    MethodNotAllowed,
    /// The host is not in the fleet's network allowlist.
    HostNotAllowed,
    /// The host is, or resolves to, an address this runner never reaches.
    AddressNotAllowed,
    /// A placeholder, or a header, stands where none may.
    PlacementNotAllowed,
    /// A credential named by a placeholder may not be sent to this host.
    CredentialHostNotAllowed,
    /// A placeholder names a secret this fleet does not have.
    SecretNotFound,
    /// The host's origin rules admit no request of this shape.
    RequestPolicyNotAllowed,
    /// The daemon would not mint the credential, or its token could not be
    /// masked.
    CredentialMintRefused,
    /// The request left and no answer came back.
    UpstreamUnreachable,
}
