//! Running one command inside a sandbox and reading what it said.

pub(crate) use afr_engine::conformance::{expect, run, shell};

/// A single-threaded runtime: the trials drive one sandbox at a time.
pub(crate) fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap_or_else(|error| unreachable!("a runtime always builds here: {error}"))
}
