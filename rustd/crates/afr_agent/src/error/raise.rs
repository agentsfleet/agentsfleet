//! How a failure becomes an [`Error`](super::Error): the lifts.

use super::{Error, ErrorKind};

// Every lift is a `From`, so `?` does the conversion and no `map_err` appears
// on a path that adds nothing (`docs/RUST_ERROR_STANDARD.md` rule 2).
afd_core::error_lifts!(Error, ErrorKind:
    afr_providers::Error => Provider,
    afr_tools::Error => Tools,
    afr_secrets::Error => Scrub,
);

// The scripted engine's one lift, compiled only with it. In a module of its
// own because a `cfg` on the macro call itself leaves the lift out of a build
// that turns the feature on through a dependant, and the lift is the same
// generated `From` every other one is.
#[cfg(feature = "test-util")]
mod executor {
    use super::{Error, ErrorKind};

    afd_core::error_lifts!(Error, ErrorKind: afr_executor::Error => Executor,);
}
