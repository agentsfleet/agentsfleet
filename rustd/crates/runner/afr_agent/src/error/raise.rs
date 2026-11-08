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
