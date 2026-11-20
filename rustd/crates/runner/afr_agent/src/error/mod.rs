//! What an agent engine refuses, and what it reports.
//!
//! One error type with `pub type Result<T, E = Error>` beside it, under the
//! `af_core::error_shell!` hull every `rustd` crate carries: the boxed kind
//! keeps `Result` pointer-sized on the `Ok` path, and the captured backtrace,
//! the `[CODE]` rendering and the self-skipping `source()` are generated.
//!
//! Recovery refusals use the shared checkpoint codes; other engine failures
//! retain their source's code or the existing internal-operation classification.

use af_core::error_code::{self, Coded, ErrorCode};

mod raise;
pub(crate) use raise::{recovery_invalid, recovery_source, setup_assignment};

af_core::error_shell!(
    /// An agent-engine failure, with the backtrace of where it was raised.
    pub struct Error(ErrorKind);
);

/// Every way this crate fails.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    /// A private root cannot be restored without changing its meaning.
    #[error("run recovery is incompatible: {reason}")]
    RecoveryInvalid { reason: &'static str },
    /// Present private bytes cannot be decoded, with the original failure retained.
    #[error("run recovery is unreadable: {context}")]
    RecoverySource {
        context: &'static str,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    /// A restored root breaks its declared bounds.
    #[error("run recovery state breaks a declared bound")]
    RecoveryBounds {
        #[source]
        source: garde::Report,
    },
    /// Current memory grants or overlay validation refuse a saved root.
    #[error("run recovery memory cannot be restored")]
    RecoveryMemory {
        #[source]
        source: afr_memory::Error,
    },
    /// No authoritative, bounded task decision is available at this boundary.
    #[error("durable task preparation could not establish current authority")]
    TaskDecision,
    /// A bounded task operation failed with its original cause retained.
    #[error("durable task boundary failed: {context}")]
    TaskBoundary {
        context: &'static str,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    /// The existing native verb already retains its refusal code.
    #[error("native task request was not answered: {reason:?}")]
    TaskNative {
        reason: afr_tools::verbs::Unanswered,
    },
    /// A private bounded task document could not be encoded or decoded.
    #[error("private task document is unreadable")]
    TaskJson {
        #[from]
        source: serde_json::Error,
    },
    /// Native operation identity could not be generated.
    #[error("task cryptographic operation failed")]
    TaskCrypto {
        #[from]
        source: af_crypto::error::Error,
    },
    /// The existing context owner could not prepare complete evidence.
    #[error("task evidence context is unavailable")]
    TaskContext {
        #[from]
        source: af_context::Error,
    },
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
        source: af_providers::Error,
    },

    /// The secret scrub could not be built over the run's secret values.
    #[error("the secret scrub could not be built")]
    Scrub {
        /// The scrub's refusal.
        #[from]
        source: af_secrets::Error,
    },

    /// The catalog refused the run's tools.
    #[error("the run's tools were refused")]
    Tools {
        /// The catalog's refusal, naming the tool.
        #[from]
        source: afr_tools::Error,
    },

    /// A setup run's draft context could not be read or saved through the
    /// daemon's fenced context verb.
    #[error("the setup run's draft context could not be read or saved")]
    SetupContext {
        /// The verb's registry code, kept from its own failure.
        code: ErrorCode,
        /// The verb's failure.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// A setup run's assignment lacks what a setup run is built from.
    #[error("the setup run's assignment is unusable: {reason}")]
    SetupAssignment { reason: &'static str },
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

    /// A setup run's context verb that failed, for `failure`, which keeps
    /// its own registry code.
    #[must_use]
    pub fn setup_context(failure: impl Coded + Send + Sync + 'static) -> Self {
        Self::from(ErrorKind::SetupContext {
            code: failure.code(),
            source: Box::new(failure),
        })
    }

    /// The registry code this failure is logged under.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self.kind() {
            ErrorKind::RecoverySource { .. }
            | ErrorKind::RecoveryInvalid { .. }
            | ErrorKind::RecoveryBounds { .. }
            | ErrorKind::RecoveryMemory { .. } => error_code::RUN_CHECKPOINT_INCOMPATIBLE,
            ErrorKind::TaskDecision | ErrorKind::TaskBoundary { .. } => {
                error_code::INTERNAL_OPERATION_FAILED
            }
            ErrorKind::TaskNative {
                reason: afr_tools::verbs::Unanswered::Refused(Some(code)),
            } => *code,
            ErrorKind::TaskNative { .. } | ErrorKind::TaskJson { .. } => {
                error_code::INTERNAL_OPERATION_FAILED
            }
            ErrorKind::TaskCrypto { source } => source.code(),
            ErrorKind::TaskContext { source } => source.code(),
            ErrorKind::Scrub { .. } => error_code::INTERNAL_OPERATION_FAILED,
            #[cfg(feature = "test-util")]
            ErrorKind::Executor { .. } => error_code::INTERNAL_OPERATION_FAILED,
            ErrorKind::Checkpoint { code, .. } | ErrorKind::SetupContext { code, .. } => *code,
            ErrorKind::SetupAssignment { .. } => error_code::INTERNAL_OPERATION_FAILED,
            ErrorKind::Provider { source } => source.code(),
            ErrorKind::Tools { source } => source.code(),
        }
    }

    /// What a refused run named that this engine cannot host, for its log
    /// line; none for a failure that refused nothing.
    #[must_use]
    pub fn unhosted(&self) -> Option<Unhosted<'_>> {
        match self.kind() {
            ErrorKind::RecoverySource { .. }
            | ErrorKind::RecoveryInvalid { .. }
            | ErrorKind::RecoveryBounds { .. }
            | ErrorKind::RecoveryMemory { .. } => None,
            ErrorKind::TaskDecision | ErrorKind::TaskBoundary { .. } => None,
            ErrorKind::TaskNative { .. }
            | ErrorKind::TaskJson { .. }
            | ErrorKind::TaskCrypto { .. }
            | ErrorKind::TaskContext { .. } => None,
            ErrorKind::Tools { source } => source.unhosted_tool().map(Unhosted::Tool),
            ErrorKind::Provider { source } => source.unhosted_provider().map(Unhosted::Provider),
            ErrorKind::Checkpoint { .. }
            | ErrorKind::Scrub { .. }
            | ErrorKind::SetupContext { .. }
            | ErrorKind::SetupAssignment { .. } => None,
            #[cfg(feature = "test-util")]
            ErrorKind::Executor { .. } => None,
        }
    }
}

/// What a refused run named that the engine cannot host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unhosted<'a> {
    /// A tool with no handler in the catalog.
    Tool(&'a str),
    /// A model provider with no wire.
    Provider(&'a str),
}

#[cfg(test)]
mod tests;
