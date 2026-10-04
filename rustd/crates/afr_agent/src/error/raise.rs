//! How a failure becomes an [`Error`](super::Error): the lifts.

use super::{Error, ErrorKind};

// Every lift is a `From`, so `?` does the conversion and no `map_err` appears
// on a path that adds nothing (`docs/RUST_ERROR_STANDARD.md` rule 2).
afd_core::error_lifts!(Error, ErrorKind:
    afr_providers::Error => Provider,
    afr_tools::Error => Tools,
    afr_secrets::Error => Scrub,
);

// The scripted engine's one lift, compiled only with it.
#[cfg(feature = "test-util")]
impl From<afr_executor::Error> for Error {
    fn from(source: afr_executor::Error) -> Self {
        Self::from(ErrorKind::Executor { source })
    }
}
