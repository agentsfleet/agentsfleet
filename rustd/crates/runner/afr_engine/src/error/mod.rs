//! What the seam refuses, and how an engine's own failure crosses it.
//!
//! One error type with `pub type Result<T, E = Error>` beside it, under the
//! `af_core::error_shell!` hull every `rustd` crate carries: the boxed kind
//! keeps `Result` pointer-sized on the `Ok` path, and the captured backtrace,
//! the `[CODE]` rendering and the self-skipping `source()` are generated.
//!
//! The kinds here are the ones a caller of the seam branches on, or that
//! every engine raises alike: a host that cannot build a sandbox, a cap or a
//! park an engine has no mechanism for, a build of unknown outcome, a second
//! build for a live identifier, a refused egress plan. An engine's or a
//! building block's own failure crosses the seam whole, as an [`EngineFailure`]: its
//! code, its sentence and its causes, and the seam refusal it carries when
//! that is what it was.
//!
//! # Which codes, and why none are new
//!
//! A runner failure is read by an operator on the host's journal, never by a
//! tenant or an API client, so it reuses the registry's existing codes the way
//! `afd_bench` does (`docs/RUST_ERROR_STANDARD.md`); the `event` field on the
//! log line says which failure it was. Minting a `UZ-RUN-*` code would publish
//! it in `public/openapi.json` for a condition no client can observe.

use af_core::error_code::{self, ErrorCode};

mod egress;
mod fault;
mod raise;

pub use self::egress::EgressRefusal;
pub use self::fault::{EngineFailure, Fault};
#[cfg(feature = "test-util")]
pub use self::raise::ambiguous;
pub use self::raise::{
    egress_refused, refused, sandbox_exists, snapshot_refused, unconfined, unsupported,
};

af_core::error_shell!(
    /// A sandbox failure, with the backtrace of where it was raised.
    pub struct Error(ErrorKind);
);

/// Every way the seam fails.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    /// A filesystem or socket call failed.
    #[error("an input/output call failed")]
    Io {
        /// The operating system's reason.
        #[source]
        source: std::io::Error,
    },

    /// The host lacks something every sandbox needs, so no run runs here.
    #[error("this host cannot build a sandbox: {missing} is unavailable")]
    Refused {
        /// The missing mechanism, as the capability report names it.
        missing: &'static str,
    },

    /// A task the engine ran did not finish: it panicked or was cancelled.
    #[error("a task did not finish")]
    Task {
        /// The runtime's reason.
        #[source]
        source: tokio::task::JoinError,
    },

    /// The guest agent inside the sandbox failed.
    #[error("the sandbox's guest agent failed")]
    GuestAgent {
        /// The guest agent's failure.
        #[source]
        source: af_guest_wire::Error,
    },

    /// A sandbox still holds or allows something it must not.
    #[error("the sandbox is not confined: {detail}")]
    Unconfined {
        /// What remained.
        detail: &'static str,
    },

    /// The operating system refused the entropy an identifier needs.
    #[error("an identifier could not be minted")]
    Entropy {
        /// Why.
        #[source]
        source: af_crypto::error::Error,
    },

    /// The engine has no mechanism for what was asked.
    #[error("the {engine} engine cannot {operation}")]
    Unsupported {
        /// The engine's name.
        engine: &'static str,
        /// What was asked of it.
        operation: &'static str,
    },

    /// A build or teardown whose outcome the engine cannot tell: something
    /// may be left, and only a sweep may touch it.
    #[cfg_attr(
        not(feature = "test-util"),
        expect(
            dead_code,
            reason = "the Firecracker engine raises it; until it lands only test engines do"
        )
    )]
    #[error("the outcome of {operation} for sandbox {sandbox} is unknown")]
    Ambiguous {
        /// The sandbox's identifier.
        sandbox: String,
        /// What was under way.
        operation: &'static str,
    },

    /// A build named a sandbox that is already live.
    #[error("sandbox {sandbox} is already live")]
    SandboxExists {
        /// The sandbox's identifier.
        sandbox: String,
    },

    /// A run's egress cannot be held to what its policy allows, so the run
    /// is refused before its sandbox runs anything.
    #[error("the run's egress was refused: {refusal}")]
    EgressRefused {
        /// Why.
        refusal: EgressRefusal,
    },

    /// A snapshot was offered to a sandbox it does not belong to.
    #[error("the snapshot names another {refusal} than the sandbox's")]
    SnapshotRefused {
        /// What differs.
        refusal: SnapshotRefusal,
    },

    /// The chunk store a snapshot is published into or read from failed.
    #[error("the snapshot's chunk store failed")]
    ChunkStore {
        /// The store's failure.
        #[source]
        source: af_chunk_store::Error,
    },

    /// An engine's or a building block's own failure, told as itself.
    #[error(transparent)]
    Engine {
        /// The failure, with its own code and causes.
        source: Fault,
    },
}

/// The one alias every signature in this crate spells.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// What a snapshot names that the sandbox offered it does not hold, as a log
/// line and a test read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::Display, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum SnapshotRefusal {
    /// Another engine, root or guest agent: what it saved runs nowhere else.
    Environment,
    /// Another generation than the store names under its identifier.
    Generation,
}

impl Error {
    /// The registry code this failure is logged under.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self.kind() {
            ErrorKind::GuestAgent { source } => source.code(),
            ErrorKind::ChunkStore { source } => source.code(),
            ErrorKind::Engine { source } => source.code(),
            ErrorKind::Io { .. }
            | ErrorKind::Refused { .. }
            | ErrorKind::Task { .. }
            | ErrorKind::Unconfined { .. }
            | ErrorKind::Entropy { .. }
            | ErrorKind::Unsupported { .. }
            | ErrorKind::Ambiguous { .. }
            | ErrorKind::SandboxExists { .. }
            | ErrorKind::EgressRefused { .. }
            | ErrorKind::SnapshotRefused { .. } => error_code::INTERNAL_OPERATION_FAILED,
        }
    }

    /// The failure in its own sentence, without its code or backtrace: a
    /// kind told as the error it carries is told as that error's sentence.
    #[must_use]
    pub fn detail(&self) -> String {
        format!("{:#}", self.kind())
    }

    /// The seam refusal this failure is, or carries from the engine that
    /// raised it: what [`Error::is_ambiguous`] and its siblings read.
    fn seam(&self) -> &Self {
        match self.kind() {
            ErrorKind::Engine { source } => source.seam().map_or(self, Self::seam),
            ErrorKind::Io { .. }
            | ErrorKind::Refused { .. }
            | ErrorKind::Task { .. }
            | ErrorKind::GuestAgent { .. }
            | ErrorKind::Unconfined { .. }
            | ErrorKind::Entropy { .. }
            | ErrorKind::Unsupported { .. }
            | ErrorKind::Ambiguous { .. }
            | ErrorKind::SandboxExists { .. }
            | ErrorKind::EgressRefused { .. }
            | ErrorKind::SnapshotRefused { .. }
            | ErrorKind::ChunkStore { .. } => self,
        }
    }

    /// Whether the engine could not tell how a build or teardown ended, so
    /// the caller must sweep the sandbox and never build it again.
    #[must_use]
    pub fn is_ambiguous(&self) -> bool {
        matches!(self.seam().kind(), ErrorKind::Ambiguous { .. })
    }

    /// The mechanism this host lacks, when the failure is that the host cannot
    /// build a sandbox at all — as opposed to one sandbox failing.
    #[must_use]
    pub fn missing_mechanism(&self) -> Option<&'static str> {
        match self.seam().kind() {
            ErrorKind::Refused { missing } => Some(missing),
            _other => None,
        }
    }

    /// What was asked of an engine with no mechanism for it, when that is the
    /// failure: `park`, or the cap it cannot `limit`.
    #[must_use]
    pub fn unsupported_operation(&self) -> Option<&'static str> {
        match self.seam().kind() {
            ErrorKind::Unsupported { operation, .. } => Some(operation),
            _other => None,
        }
    }

    /// Why the run's egress was refused, when that is the failure.
    #[must_use]
    pub fn egress_refusal(&self) -> Option<&EgressRefusal> {
        match self.seam().kind() {
            ErrorKind::EgressRefused { refusal } => Some(refusal),
            _other => None,
        }
    }

    /// What a refused snapshot names that the sandbox does not hold, when that
    /// is the failure.
    #[must_use]
    pub fn snapshot_refusal(&self) -> Option<SnapshotRefusal> {
        match self.seam().kind() {
            ErrorKind::SnapshotRefused { refusal } => Some(*refusal),
            _other => None,
        }
    }

    /// An engine's or a building block's own `failure`, carried across the
    /// seam with its code, its sentence and its causes.
    #[must_use]
    pub fn engine(failure: impl EngineFailure) -> Self {
        ErrorKind::Engine {
            source: Fault::new(failure),
        }
        .into()
    }
}

#[cfg(test)]
mod tests;
