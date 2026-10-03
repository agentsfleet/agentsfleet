//! The agent engine: what runs one lease's turn.
//!
//! [`AgentEngine`] is the seam the supervisor drives. It receives the lease,
//! the hydrated memory, the sandbox's executor when the lease has one, and an
//! [`EventSink`] its activity goes to; it returns the result the report
//! carries and the memory to push. The agent loop and its providers implement
//! it in their own workstream; a scripted engine drives the tests here.

pub mod error;

mod engine;
#[cfg(feature = "test-util")]
pub mod scripted;

pub use self::engine::{AgentEngine, AgentRun, EventSink, RunOutput};
pub use self::error::{Error, Result};
