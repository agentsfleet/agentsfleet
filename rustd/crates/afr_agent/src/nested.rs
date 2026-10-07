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

#[path = "nested/answer.rs"]
mod answer;
#[path = "nested/child.rs"]
mod child;
#[path = "nested/delegate.rs"]
mod delegate;
#[path = "nested/input.rs"]
mod input;
#[path = "nested/interrupt.rs"]
mod interrupt;
#[path = "nested/list.rs"]
mod list;
#[path = "nested/registry.rs"]
mod registry;
#[path = "nested/spawn.rs"]
mod spawn;
#[path = "nested/start.rs"]
mod start;
#[path = "nested/wait.rs"]
mod wait;

#[cfg(test)]
#[path = "nested/fixture.rs"]
mod fixture;

#[cfg(test)]
#[path = "nested/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "nested/refusal_tests.rs"]
mod refusal_tests;

#[cfg(test)]
#[path = "nested/run_tests.rs"]
mod run_tests;

#[cfg(test)]
#[path = "nested/end_tests.rs"]
mod end_tests;

#[cfg(test)]
#[path = "nested/interrupt_tests.rs"]
mod interrupt_tests;

#[cfg(test)]
#[path = "nested/answer_tests.rs"]
mod answer_tests;

#[cfg(test)]
#[path = "nested/lifetime_tests.rs"]
mod lifetime_tests;
