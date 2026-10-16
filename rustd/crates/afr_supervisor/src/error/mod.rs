//! What the supervisor refuses, and what it reports.
//!
//! One error type with `pub type Result<T, E = Error>` beside it, under the
//! `afd_core::error_shell!` hull every `rustd` crate carries: the boxed kind
//! keeps `Result` pointer-sized on the `Ok` path, and the captured backtrace,
//! the `[CODE]` rendering and the self-skipping `source()` are generated.
//!
//! # Three classes, never one
//!
//! A daemon call ends one of three ways and each is handled differently
//! (RULE ECL): it never arrived or the daemon was briefly unwell — a transport
//! failure, a 5xx, a 429 — and may be retried; the daemon refused it with a
//! 4xx, which no retry changes; or the reply did not decode. Collapsing them is
//! how a runner either busy-loops against a refusal or gives up on a blip.
//!
//! # Which codes, and why none are new
//!
//! A runner failure is read by an operator on the host's journal, never by a
//! tenant or an API client, so it reuses the registry's existing codes the way
//! `afd_bench` does (`docs/RUST_ERROR_STANDARD.md`); the `event` field on the
//! log line says which failure it was. A refusal carries the daemon's own code
//! through, so the log names exactly what the daemon said.

use afd_core::error_code::ErrorCode;
use reqwest::StatusCode;

use crate::client::Verb;

mod raise;

#[cfg(test)]
pub(crate) use self::raise::refused;
pub(crate) use self::raise::{
    client, config, egress, egress_blocked, egress_no_ipv4, egress_unresolved, encode, git,
    malformed, refused_with_body, token_refused, transport, unavailable,
};

/// The daemon refused the runner's token.
pub(crate) const UNAUTHORIZED: u16 = StatusCode::UNAUTHORIZED.as_u16();

afd_core::error_shell!(
    /// A supervisor failure, with the backtrace of where it was raised.
    pub struct Error(ErrorKind);
);

/// Every way the supervisor fails.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    /// A filesystem call failed.
    #[error("an input/output call failed")]
    Io {
        /// The operating system's reason.
        #[source]
        source: std::io::Error,
    },

    /// Private spool encryption or authentication failed.
    #[error("private report protection failed")]
    Crypto {
        #[source]
        source: afd_crypto::error::Error,
    },

    /// A finished temporary file could not be moved into place.
    #[error("a finished file could not be moved into place")]
    Persist {
        /// The rename's failure; the temporary file is removed with it.
        #[source]
        source: tempfile::PersistError,
    },

    /// The runner's configuration is missing or unusable.
    #[error("{detail}")]
    Config {
        /// Which setting, and what is wrong with it.
        detail: &'static str,
    },

    /// The daemon's address does not parse, or a path does not join it.
    #[error("the daemon's address is not usable")]
    Address {
        /// The parser's reason.
        #[source]
        source: url::ParseError,
    },

    /// The HTTP client could not be built: no usable TLS backend.
    #[error("the HTTP client could not be built")]
    Client {
        /// The client builder's reason.
        #[source]
        source: reqwest::Error,
    },

    /// The call never reached the daemon, or its reply never arrived whole.
    #[error("the {verb} call did not reach the daemon")]
    Transport {
        /// Which verb.
        verb: Verb,
        /// The client's reason.
        #[source]
        source: reqwest::Error,
    },

    /// The daemon answered 5xx or 429: try again later.
    #[error("the daemon could not serve the {verb} call ({status})")]
    Unavailable {
        /// Which verb.
        verb: Verb,
        /// The status it answered.
        status: u16,
    },

    /// The daemon refused the call with a 4xx; retrying changes nothing.
    #[error("the daemon refused the {verb} call ({status})")]
    Refused {
        /// Which verb.
        verb: Verb,
        /// The status it answered.
        status: u16,
        /// The registry code its problem body named, when it named one.
        code: Option<ErrorCode>,
    },

    /// The runner stopped because the daemon refused its token.
    #[error("the daemon refused this runner's token, so the runner stopped")]
    TokenRefused,

    /// The daemon's reply did not decode as the verb's reply shape.
    #[error("the daemon's {verb} reply did not decode")]
    Malformed {
        /// Which verb.
        verb: Verb,
        /// The decoder's reason.
        #[source]
        source: serde_json::Error,
    },

    /// A body this runner built would not serialize.
    #[error("a request body would not serialize")]
    Encode {
        /// The encoder's reason.
        #[source]
        source: serde_json::Error,
    },

    /// A task the supervisor ran did not finish: it panicked or was cancelled.
    #[error("a task did not finish")]
    Task {
        /// The runtime's reason.
        #[source]
        source: tokio::task::JoinError,
    },

    /// A repository would not fetch into its mirror, or would not check out
    /// into the workspace.
    #[error("repository {repository} could not be {step}")]
    Git {
        /// The repository, as `owner/name`.
        repository: String,
        /// What was being done when it failed.
        step: &'static str,
        /// The git library's reason.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// A lease asked for a sandbox size outside the wire's bounds.
    #[error("the lease asked for a sandbox size this runner will not build")]
    LeaseSize {
        /// Which bound it broke.
        #[source]
        source: garde::Report,
    },

    /// An identifier the daemon sent is not in canonical form.
    #[error("the daemon sent an identifier this runner cannot read")]
    Identifier {
        /// The parser's reason.
        #[source]
        source: afd_core::error::Error,
    },

    /// A host the lease's egress allowlist names did not resolve.
    #[error("egress host {host} could not be resolved")]
    EgressUnresolved {
        /// The host, as the allowlist names it.
        host: String,
        /// The resolver's reason.
        #[source]
        source: std::io::Error,
    },

    /// A host the lease's egress allowlist names resolved to no IPv4 address,
    /// and only an IPv4 address can be admitted.
    #[error("egress host {host} resolves to no IPv4 address")]
    EgressNoIpv4 {
        /// The host, as the allowlist names it.
        host: String,
    },

    /// A host the fleet's `network.allow` names resolved to an address in
    /// `afd_core::net`'s blocked ranges: loopback, private, shared,
    /// link-local or reserved. The address stays out of the text, as it does
    /// out of every egress log.
    #[error(
        "egress host {host} resolves to a private or reserved address, which no fleet may reach"
    )]
    EgressBlocked {
        /// The host, as the allowlist names it.
        host: String,
    },

    /// The sandbox engine would not take the lease's resolved allowlist.
    #[error("the lease's egress allowlist was refused")]
    Egress {
        /// The engine's reason.
        #[source]
        source: afr_sandbox::Error,
    },
}

/// The one alias every signature in this crate spells.
pub type Result<T, E = Error> = std::result::Result<T, E>;

mod classify;

#[cfg(test)]
mod tests;
