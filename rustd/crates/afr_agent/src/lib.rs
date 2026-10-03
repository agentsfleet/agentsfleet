//! The agent engine: what runs one lease's turn.
//!
//! [`AgentEngine`] is the seam the supervisor drives. It receives the lease,
//! the hydrated memory, the sandbox's executor when the lease has one, and an
//! [`EventSink`] its activity goes to; it returns the result the report
//! carries and the memory to push. Before any of that, [`AgentEngine::admit`]
//! refuses a lease naming a tool the engine cannot host and says whether the
//! lease needs a sandbox. The [`Router`] runs each tool call where its runtime
//! says; a scripted engine drives the supervisor's tests.

pub mod error;

mod engine;
mod router;
#[cfg(feature = "test-util")]
pub mod scripted;

pub use self::engine::{AgentEngine, AgentRun, EventSink, Needs, RunOutput};
pub use self::error::{Error, Result};
pub use self::router::Router;
