//! The agent engine: what runs one run's turn.
//!
//! [`AgentEngine`] is the seam the supervisor drives. It receives the run,
//! the loaded memory, the sandbox's executor when the run has one, and an
//! [`EventSink`] its activity goes to; it returns the result the report
//! carries and the memory to push. Before any of that, [`AgentEngine::admit`]
//! refuses a run naming a tool the engine cannot host and says whether the
//! run needs a sandbox. [`Loop`] is the engine that runs the model against the
//! run's tools: each loop's router runs a call through `guffbolt`'s tool set,
//! a catalog handler where its runtime says. A scripted engine drives the
//! supervisor's lanes. [`AgentEngine::setup`] runs a setup draft's turn on the
//! same loop, with no sandbox and `propose_setup` alone ([`setup`]).

#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

pub mod error;

mod context;
mod decision;
mod engine;
mod events;
#[cfg(test)]
mod fixture;
#[path = "loop/mod.rs"]
mod harness;
mod ledger;
mod offer;
mod prompt;
mod records;
mod result;
mod router;
#[cfg(feature = "test-util")]
pub mod scripted;
pub mod setup;
mod spans;
#[cfg(any(test, feature = "test-util"))]
pub mod testing;
mod trace;

pub use self::engine::{
    AgentEngine, AgentRun, Checkpoint, CommittedRecovery, EventSink, FileRecoveryPermit, Folded,
    Inbox, Meter, Needs, RecoveryBoundarySink, RecoveryReceipt, RecoverySave, RecoverySkip,
    RecoverySnapshot, RecoveryState, RunOutput, TurnRecovery, TurnSnapshot,
};
pub use self::error::{Error, Result, Unhosted};
pub use self::harness::Loop;
pub use self::result::{Completed, ExecutionResult, Failure, ResultOutcome};
pub use self::setup::{SetupContext, SetupOutput, SetupRun};

pub use decision::PreparedTask;
pub use engine::TaskRecovery;
