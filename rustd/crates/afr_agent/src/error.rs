//! What an agent engine refuses, and what it reports.
//!
//! One error type with `pub type Result<T, E = Error>` beside it, under the
//! `afd_core::error_shell!` hull every `rustd` crate carries: the boxed kind
//! keeps `Result` pointer-sized on the `Ok` path, and the captured backtrace,
//! the `[CODE]` rendering and the self-skipping `source()` are generated.
//!
//! # Which codes, and why none are new
//!
//! A runner failure is read by an operator on the host's journal, never by a
//! tenant or an API client, so it reuses the registry's existing codes the way
//! `afd_bench` does (`docs/RUST_ERROR_STANDARD.md`); the `event` field on the
//! log line says which failure it was. Minting a `UZ-RUN-*` code would publish
//! it in `public/openapi.json` for a condition no client can observe.

use afd_core::error_code::{self, ErrorCode};

mod raise;

afd_core::error_shell!(
    /// An agent-engine failure, with the backtrace of where it was raised.
    pub struct Error(ErrorKind);
);

/// Every way this crate fails.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    /// The sandbox's executor failed under a call the run made.
    #[error("the executor failed")]
    Executor {
        /// The executor's failure.
        #[from]
        source: afr_executor::Error,
    },

    /// The model provider the policy names could not be reached.
    #[error("the model provider could not be reached")]
    Provider {
        /// The provider's failure.
        #[from]
        source: afr_providers::Error,
    },

    /// The secret scrub could not be built over the run's secret values.
    #[error("the secret scrub could not be built")]
    Scrub {
        /// The matcher's refusal.
        #[from]
        source: aho_corasick::BuildError,
    },

    /// The catalog refused the lease's tools.
    #[error("the lease's tools were refused")]
    Tools {
        /// The catalog's refusal, naming the tool.
        #[from]
        source: afr_tools::Error,
    },
}

/// The one alias every signature in this crate spells.
pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    /// The registry code this failure is logged under.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        match self.kind() {
            ErrorKind::Executor { .. } | ErrorKind::Scrub { .. } => {
                error_code::INTERNAL_OPERATION_FAILED
            }
            ErrorKind::Provider { source } => source.code(),
            ErrorKind::Tools { source } => source.code(),
        }
    }

    /// What a refused lease named that this engine cannot host, for its log
    /// line; none for a failure that refused nothing.
    #[must_use]
    pub fn unhosted(&self) -> Option<Unhosted<'_>> {
        match self.kind() {
            ErrorKind::Tools { source } => source.unhosted_tool().map(Unhosted::Tool),
            ErrorKind::Provider { source } => source.unhosted_provider().map(Unhosted::Provider),
            ErrorKind::Executor { .. } | ErrorKind::Scrub { .. } => None,
        }
    }
}

/// What a refused lease named that the engine cannot host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unhosted<'a> {
    /// A tool with no handler in the catalog.
    Tool(&'a str),
    /// A model provider with no wire.
    Provider(&'a str),
}

#[cfg(test)]
#[path = "error/tests.rs"]
mod tests;
