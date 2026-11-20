//! How a failure becomes an [`Error`](super::Error): the lifts.

use super::{Error, ErrorKind};

// Every lift is a `From`, so `?` does the conversion and no `map_err` appears
// on a path that adds nothing (`docs/RUST_ERROR_STANDARD.md` rule 2).
af_core::error_lifts!(Error, ErrorKind:
    af_providers::Error => Provider,
    afr_tools::Error => Tools,
    af_secrets::Error => Scrub,
    serde_json::Error => TaskJson,
    af_crypto::error::Error => TaskCrypto,
    af_context::Error => TaskContext,
    garde::Report => RecoveryBounds,
    afr_memory::Error => RecoveryMemory,
);

// The scripted engine's one lift, compiled only with it. In a module of its
// own because a `cfg` on the macro call itself leaves the lift out of a build
// that turns the feature on through a dependant, and the lift is the same
// generated `From` every other one is.
#[cfg(feature = "test-util")]
mod executor {
    use super::{Error, ErrorKind};

    af_core::error_lifts!(Error, ErrorKind: afr_executor::Error => Executor,);
}

/// A saved root cannot be resumed under its declared meaning.
pub(crate) fn recovery_invalid(reason: &'static str) -> Error {
    ErrorKind::RecoveryInvalid { reason }.into()
}

/// Preserve the decoding cause with the private document being restored.
pub(crate) fn recovery_source(
    context: &'static str,
    source: impl std::error::Error + Send + Sync + 'static,
) -> Error {
    ErrorKind::RecoverySource {
        context,
        source: Box::new(source),
    }
    .into()
}

/// A setup run's assignment that lacks what a setup run is built from.
pub(crate) fn setup_assignment(reason: &'static str) -> Error {
    ErrorKind::SetupAssignment { reason }.into()
}
