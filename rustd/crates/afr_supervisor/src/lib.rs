//! The supervisor: the trusted half of the runner, outside every sandbox.
//!
//! It speaks the runner verbs through [`afd_wire`] and keeps every duty the
//! daemon relies on — the worker pool, renewal, the report spooled before it is
//! posted, the bounded activity sender, credential minting, memory hydrate and
//! fenced push, bundle fetch, the boot sweep and the capability report
//! (`docs/architecture/runner_fleet.md` §The control protocol). Model keys live
//! only here; no sandbox ever holds one.

pub mod error;

pub use self::error::{Error, Result};
