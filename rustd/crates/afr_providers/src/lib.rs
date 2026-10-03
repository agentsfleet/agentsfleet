//! The model providers: one [`Provider`] trait, the turn it streams, and the
//! wires behind it (`docs/architecture/runner_execution.md` §Crates).
//!
//! The model key lives only here, in the supervisor: no provider runs inside a
//! sandbox.

pub mod error;

mod provider;

pub use self::error::{Error, Result};
pub use self::provider::{Call, Chunk, Message, Provider, Request, Usage};
