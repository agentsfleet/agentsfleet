//! Nested loops: `delegate`, `spawn`, `wait_agent`, `send_input`,
//! `list_agents` and `interrupt_agent`, run by the loop beside the turn.
//!
//! A child is the run's loop over a task of the model's, with a selection
//! narrowed from its parent's: the same provider and key, sandbox, workspace,
//! memory, call counter and budget, and nothing of its own but its
//! conversation. The registry keeps every child's state, inbox and stop; the
//! root loop polls every child's future. A child at the depth cap is offered
//! none of the six, and the run's caps refuse a fifth running or a
//! seventeenth started child (`docs/architecture/runner_execution.md` §"Tool
//! catalog").

use afr_providers::Call;
use afr_tools::ToolOutput;
use afr_tools::nested::Nested;

pub(crate) use self::child::{Guard, Tether, run as child};
pub(crate) use self::registry::{Registry, Seat, Start};
use crate::harness::Harness;

/// Runs `call` when it names one of the six this loop was offered; `None`
/// for any other call, which is the router's.
pub(crate) async fn run(harness: &Harness<'_, '_>, call: &Call) -> Option<ToolOutput> {
    let nested = Nested::of(&call.name).filter(|_| harness.selection.tool(&call.name).is_some())?;
    let arguments = &call.arguments;
    Some(match nested {
        Nested::Delegate => delegate::run(harness, arguments).await,
        Nested::Spawn => spawn::run(harness, arguments),
        Nested::WaitAgent => wait::run(harness, arguments).await,
        Nested::SendInput => input::run(harness, arguments),
        Nested::ListAgents => list::run(harness, arguments),
        Nested::InterruptAgent => interrupt::run(harness, arguments),
    })
}

mod answer;
mod child;
mod delegate;
mod input;
mod interrupt;
mod list;
mod registry;
mod spawn;
mod start;
mod wait;

#[cfg(test)]
mod fixture;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod refusal_tests;

#[cfg(test)]
mod run_tests;

#[cfg(test)]
mod end_tests;

#[cfg(test)]
mod interrupt_tests;

#[cfg(test)]
mod answer_tests;

#[cfg(test)]
mod lifetime_tests;

#[cfg(test)]
mod registry_tests;
