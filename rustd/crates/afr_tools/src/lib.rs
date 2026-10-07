//! The tool catalog: every tool the published tools page names, the runtime
//! each executes in, and the handlers this runner hosts.
//!
//! The runner is the harness in Codex's shape: a lease's
//! `ExecutionPolicy.tools` selects which tools the model is offered, the model
//! picks by function calling, and the router in `afr_agent` runs each handler
//! where its [`Runtime`] says (`docs/architecture/runner_execution.md` §"Tool
//! catalog").

pub mod catalog;
pub mod error;
pub mod nested;
pub mod sandbox;
#[cfg(any(test, feature = "test-util"))]
pub mod stub;

mod egress;
mod handler;
mod http_request;
mod lease;
mod memory;
mod plan;
mod pushover;
mod runtime;
mod schema;
#[cfg(test)]
mod testing;
pub mod verbs;
mod web_fetch;

pub use self::catalog::{Catalog, Entry, Selection};
pub use self::error::{Error, Result};
pub use self::lease::Lease;
pub use self::runtime::{Runtime, Tool, ToolContext, ToolErrorCode, ToolOutput, parsed};
pub use self::schema::Schema;
#[cfg(any(test, feature = "test-util"))]
pub use self::verbs::{CLOSED, Closed};
pub use self::verbs::{LeaseVerbs, ScheduleCall, Unanswered};
