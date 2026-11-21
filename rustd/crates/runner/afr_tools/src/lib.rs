//! The tool catalog: every tool the published tools page names, the runtime
//! each executes in, and the handlers this runner hosts.
//!
//! The runner is the harness in Codex's shape: a run's
//! `ExecutionPolicy.tools` selects which tools the model is offered, the model
//! picks by function calling, and the router in `afr_agent` runs each handler
//! where its [`Runtime`] says (`docs/architecture/runner_execution.md` §"Tool
//! catalog"). The generic tools are `guffbolt`'s, each built per run over
//! its backend ([`Backends`]): the run's sandbox ([`LazySandboxEnv`]), its
//! egress guard, its memory, its verbs ([`Fenced`]), Pushover and its
//! children, whose subagent tools this catalog publishes under `guffbolt`'s
//! own names.

#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

mod backed;
pub mod catalog;
pub mod error;
pub mod sandbox;
mod selection;
pub mod setup;
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
mod pushover;
mod run;
mod runtime;
#[cfg(test)]
mod testing;
pub mod verbs;
#[cfg(test)]
mod web_fetch;

pub use self::backed::{Backends, RunTools};
pub use self::catalog::{Catalog, Entry};
pub use self::error::{Error, Result};
pub use self::generic::{LazySandboxEnv, RunMemory, SandboxEnv};
pub use self::run::Run;
pub use self::runtime::{CatalogTool, RunTool, Runtime, ToolContext, ToolErrorCode};
pub use self::selection::{Offered, Selection};
#[cfg(any(test, feature = "test-util"))]
pub use self::verbs::{CLOSED, Closed};
pub use self::verbs::{Fenced, FleetsCall, RunVerbs, ScheduleCall, Unanswered};
