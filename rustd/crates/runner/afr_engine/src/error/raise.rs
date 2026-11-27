//! How a failure becomes an [`Error`](super::Error): the lifts, and the raisers
//! that bind what only the call site knows. The raisers are public: every
//! engine and building block refuses in the seam's words.

use super::{EgressRefusal, Error, ErrorKind};

// Every lift is a `From`, so `?` does the conversion and no `map_err` appears
// on a path that adds nothing (`docs/RUST_ERROR_STANDARD.md` rule 2).
af_core::error_lifts!(Error, ErrorKind:
    std::io::Error => Io,
    af_guest_wire::Error => GuestAgent,
    tokio::task::JoinError => Task,
    af_crypto::error::Error => Entropy,
);

/// Refuses every run on this host: `missing` is what it lacks.
#[must_use]
pub fn refused(missing: &'static str) -> Error {
    ErrorKind::Refused { missing }.into()
}

/// Refuses a run whose egress cannot be held to its policy, saying why.
#[must_use]
pub fn egress_refused(refusal: EgressRefusal) -> Error {
    ErrorKind::EgressRefused { refusal }.into()
}

/// Refuses what `engine` has no mechanism for, naming the operation.
#[must_use]
pub fn unsupported(engine: af_wire::runner::SandboxEngine, operation: &'static str) -> Error {
    ErrorKind::Unsupported {
        engine: engine.into(),
        operation,
    }
    .into()
}

/// A build or teardown of `sandbox` whose outcome is unknown, for a test's
/// engine to answer; an engine that can lose track of a build raises it too.
#[cfg(feature = "test-util")]
#[must_use]
pub fn ambiguous(sandbox: &af_core::id::Uuid7, operation: &'static str) -> Error {
    ErrorKind::Ambiguous {
        sandbox: sandbox.to_string(),
        operation,
    }
    .into()
}

/// Refuses a build for a sandbox that is already live.
#[must_use]
pub fn sandbox_exists(sandbox: &af_core::id::Uuid7) -> Error {
    ErrorKind::SandboxExists {
        sandbox: sandbox.to_string(),
    }
    .into()
}

/// Refuses to serve from a sandbox that still holds or allows what it must
/// not.
#[must_use]
pub fn unconfined(detail: &'static str) -> Error {
    ErrorKind::Unconfined { detail }.into()
}
