//! The agent engine: what runs one lease's turn.
//!
//! [`AgentEngine`] is the seam the supervisor drives. It receives the lease,
//! the hydrated memory, the sandbox's executor when the lease has one, and an
//! [`EventSink`] its activity goes to; it returns the result the report
//! carries and the memory to push. Before any of that, [`AgentEngine::admit`]
//! refuses a lease naming a tool the engine cannot host and says whether the
//! lease needs a sandbox. [`Loop`] is the engine that runs the model against the
//! lease's tools, and the [`Router`] runs each call where its runtime says; a
//! scripted engine drives the supervisor's lanes.

pub mod error;

mod context;
mod engine;
mod events;
#[cfg(test)]
mod fixture;
#[path = "loop.rs"]
mod harness;
mod ledger;
mod nested;
mod offer;
mod prompt;
mod records;
mod router;
#[cfg(feature = "test-util")]
pub mod scripted;
mod spans;
#[cfg(any(test, feature = "test-util"))]
pub mod testing;
mod trace;
mod turn;

pub use self::engine::{AgentEngine, AgentRun, Checkpoint, EventSink, Meter, Needs, RunOutput};
pub use self::error::{Error, Result, Unhosted};
pub use self::harness::Loop;
pub use self::router::Router;
// The seam a run's schedule and message tools reach `agentsfleetd` through,
// re-exported so the supervisor implements it through the crate it drives.
pub use afr_tools::{LeaseVerbs, ScheduleCall, Unanswered};
