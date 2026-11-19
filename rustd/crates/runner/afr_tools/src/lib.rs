//! The tool catalog: every tool the published tools page names, the runtime
//! each executes in, and the handlers this runner hosts.
//!
//! The runner is the harness in Codex's shape: a run's
//! `ExecutionPolicy.tools` selects which tools the model is offered, the model
//! picks by function calling, and the router in `afr_agent` runs each handler
//! where its [`Runtime`] says (`docs/architecture/runner_execution.md` §"Tool
//! catalog"). The generic tools are `guffbolt`'s, lent the run's sandbox,
//! egress guard and memory through the harness's seams ([`SandboxEnv`] and
//! the adapters beside it); the tools that reach `agentsfleetd` are this
//! crate's own (`verbs`).

#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

pub mod catalog;
pub mod error;
pub mod nested;
pub mod sandbox;
mod selection;
#[cfg(any(test, feature = "test-util"))]
pub mod stub;

mod egress;
mod generic;
mod handler;
#[cfg(test)]
mod http_request;
#[cfg(test)]
mod memory;
#[cfg(test)]
mod plan;
#[cfg(test)]
mod pushover;
mod run;
mod runtime;
#[cfg(test)]
mod testing;
pub mod verbs;
#[cfg(test)]
mod web_fetch;

pub use self::catalog::{Catalog, Entry};
pub use self::error::{Error, Result};
pub use self::generic::SandboxEnv;
pub use self::run::Run;
pub use self::runtime::{CatalogTool, Runtime, ToolContext, ToolErrorCode};
pub use self::selection::{Offered, Selection};
#[cfg(any(test, feature = "test-util"))]
pub use self::verbs::{CLOSED, Closed};
pub use self::verbs::{FleetsCall, RunVerbs, ScheduleCall, Unanswered};
