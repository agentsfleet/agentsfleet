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

use afd_core::error_code::{self, ErrorCode};

use crate::client::Verb;

mod raise;

#[cfg(test)]
pub(crate) use self::raise::refused;
pub(crate) use self::raise::{
    client, config, encode, malformed, refused_with_body, tampered, transport, unavailable,
};

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
        #[from]
        source: std::io::Error,
    },

    /// A finished temporary file could not be moved into place.
    #[error("a finished file could not be moved into place")]
    Persist {
        /// The rename's failure; the temporary file is removed with it.
        #[from]
        source: tempfile::PersistError,
    },

    /// The runner's configuration is missing or unusable.
    #[error("{detail}")]
    Config {
        /// Which setting, and what is wrong with it.
        detail: &'static str,
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

    /// A fetched bundle's bytes do not hash to the name it was fetched by.
    #[error("bundle {content_hash} does not hash to its own name")]
    BundleTampered {
        /// The hash the lease named.
        content_hash: String,
    },

    /// An identifier the daemon sent is not in canonical form.
    #[error("the daemon sent an identifier this runner cannot read")]
    Identifier {
        /// The parser's reason.
        #[from]
        source: afd_core::error::Error,
    },

    /// No sandbox could be built for a lease.
    #[error("a sandbox could not be built")]
    Sandbox {
        /// The engine's reason.
        #[from]
        source: afr_sandbox::Error,
    },
}

/// The one alias every signature in this crate spells.
pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    /// Whether retrying the same call could succeed.
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        matches!(
            self.kind(),
            ErrorKind::Transport { .. } | ErrorKind::Unavailable { .. }
        )
    }

    /// The registry code the daemon refused with, when this is a refusal that
    /// named one.
    #[must_use]
    pub const fn refusal_code(&self) -> Option<ErrorCode> {
        match self.kind() {
            ErrorKind::Refused { code, .. } => *code,
            _other => None,
        }
    }

    /// Whether the daemon refused this runner's token.
    #[must_use]
    pub const fn is_unauthorized(&self) -> bool {
        matches!(self.kind(), ErrorKind::Refused { status: 401, .. })
    }

    /// The registry code this failure is logged under.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        match self.kind() {
            ErrorKind::Refused {
                code: Some(code), ..
            } => *code,
            ErrorKind::Refused { status: 401, .. } => error_code::RUN_INVALID_RUNNER_TOKEN,
            ErrorKind::Refused {
                verb: Verb::Renew, ..
            } => error_code::RUN_LEASE_LOST,
            ErrorKind::BundleTampered { .. } => error_code::FLEET_BUNDLE_INVALID,
            ErrorKind::Transport { verb, .. }
            | ErrorKind::Unavailable { verb, .. }
            | ErrorKind::Refused { verb, .. }
            | ErrorKind::Malformed { verb, .. } => verb.code(),
            ErrorKind::Io { .. }
            | ErrorKind::Persist { .. }
            | ErrorKind::Config { .. }
            | ErrorKind::Client { .. }
            | ErrorKind::Encode { .. }
            | ErrorKind::Identifier { .. }
            | ErrorKind::Sandbox { .. } => error_code::INTERNAL_OPERATION_FAILED,
        }
    }
}

#[cfg(test)]
#[path = "error/tests.rs"]
mod tests;
