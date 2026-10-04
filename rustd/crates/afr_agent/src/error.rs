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

use afd_core::error_code::{self, Coded, ErrorCode};

mod raise;

afd_core::error_shell!(
    /// An agent-engine failure, with the backtrace of where it was raised.
    pub struct Error(ErrorKind);
);

/// Every way this crate fails.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    /// A mid-run memory checkpoint was not written.
    #[error("a mid-run memory checkpoint was not written")]
    Checkpoint {
        /// The writer's registry code, kept from its own failure.
        code: ErrorCode,
        /// The writer's failure.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// The sandbox's executor failed under a call the scripted engine made;
    /// the loop routes no executor failure here, so only the lanes' engine
    /// builds it.
    #[cfg(feature = "test-util")]
    #[error("the executor failed")]
    Executor {
        /// The executor's failure.
        #[source]
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
        /// The scrub's refusal.
        #[from]
        source: afr_secrets::Error,
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
    /// A checkpoint the writer could not write, for `failure`, which keeps
    /// its own registry code.
    #[must_use]
    pub fn checkpoint(failure: impl Coded + Send + Sync + 'static) -> Self {
        Self::from(ErrorKind::Checkpoint {
            code: failure.code(),
            source: Box::new(failure),
        })
    }

    /// The registry code this failure is logged under.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        match self.kind() {
            ErrorKind::Scrub { .. } => error_code::INTERNAL_OPERATION_FAILED,
            #[cfg(feature = "test-util")]
            ErrorKind::Executor { .. } => error_code::INTERNAL_OPERATION_FAILED,
            ErrorKind::Checkpoint { code, .. } => *code,
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
            ErrorKind::Provider { source } => source
                .unhosted_provider()
                .map(Unhosted::Provider)
                .or_else(|| source.blocked_endpoint().map(Unhosted::Endpoint)),
            ErrorKind::Checkpoint { .. } | ErrorKind::Scrub { .. } => None,
            #[cfg(feature = "test-util")]
            ErrorKind::Executor { .. } => None,
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
    /// A `custom:` model endpoint at an address the runner never dials.
    Endpoint(&'a str),
}

#[cfg(test)]
#[path = "error/tests.rs"]
mod tests;
