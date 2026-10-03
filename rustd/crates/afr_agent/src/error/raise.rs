//! How a failure becomes an [`Error`](super::Error): the lifts.

use super::{Error, ErrorKind};

// Every lift is a `From`, so `?` does the conversion and no `map_err` appears
// on a path that adds nothing (`docs/RUST_ERROR_STANDARD.md` rule 2).
afd_core::error_lifts!(Error, ErrorKind:
    afr_executor::Error => Executor,
    afr_tools::Error => Tools,
);
