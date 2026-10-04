//! The catalog: every published tool and where it runs, the handlers this
//! runner hosts, and the subset one lease is offered.
//!
//! The published list is `docs/architecture/runner_execution.md` §"Tool
//! catalog", one entry per tool. A lease's `ExecutionPolicy.tools` selects from
//! it; a name with no handler here refuses the lease, never a quieter tool set,
//! the disposition `src/runner/engine/tool_bridge.zig` carries today.

use std::sync::Arc;

use afr_egress::Transport;

use crate::error::{self, Result};
use crate::handler::Typed;
use crate::http_request::HttpRequest;
use crate::memory::{MemoryForget, MemoryList, MemoryRecall, MemoryStore};
use crate::plan::UpdatePlan;
use crate::pushover::Pushover;
use crate::runtime::{Runtime, Tool};
use crate::web_fetch::WebFetch;

/// One published tool: its name and the runtime it executes in.
///
/// Built only here, so every [`Tool::entry`] names a published tool.
#[derive(Debug, PartialEq, Eq)]
pub struct Entry {
    name: &'static str,
    runtime: Runtime,
}

impl Entry {
    const fn new(name: &'static str, runtime: Runtime) -> Self {
        Self { name, runtime }
    }

    /// The name the model calls it by.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Where its handler runs.
    #[must_use]
    pub const fn runtime(&self) -> Runtime {
        self.runtime
    }
}

/// An HTTPS request under the network policy and the origin rules.
pub const HTTP_REQUEST: Entry = Entry::new("http_request", Runtime::Supervisor);
/// A credential-free `GET` of a page's text.
pub const WEB_FETCH: Entry = Entry::new("web_fetch", Runtime::Supervisor);
/// A push notification through Pushover.
pub const PUSHOVER: Entry = Entry::new("pushover", Runtime::Supervisor);
/// A web search the provider runs.
pub const WEB_SEARCH: Entry = Entry::new("web_search", Runtime::Provider);
/// Stores one durable memory item.
pub const MEMORY_STORE: Entry = Entry::new("memory_store", Runtime::Supervisor);
/// Searches the fleet's memory.
pub const MEMORY_RECALL: Entry = Entry::new("memory_recall", Runtime::Supervisor);
/// Lists the fleet's memory.
pub const MEMORY_LIST: Entry = Entry::new("memory_list", Runtime::Supervisor);
/// Forgets one memory item.
pub const MEMORY_FORGET: Entry = Entry::new("memory_forget", Runtime::Supervisor);
/// The model's plan steps, as the thread renders them.
pub const UPDATE_PLAN: Entry = Entry::new("update_plan", Runtime::Supervisor);
/// A message to the event's thread, through a runner verb.
pub const MESSAGE: Entry = Entry::new("message", Runtime::Supervisor);
/// A one-shot or recurring schedule on the daemon's plane.
pub const SCHEDULE: Entry = Entry::new("schedule", Runtime::Supervisor);
/// Adds a cron schedule.
pub const CRON_ADD: Entry = Entry::new("cron_add", Runtime::Supervisor);
/// Lists the cron schedules.
pub const CRON_LIST: Entry = Entry::new("cron_list", Runtime::Supervisor);
/// Removes a cron schedule.
pub const CRON_REMOVE: Entry = Entry::new("cron_remove", Runtime::Supervisor);
/// Updates a cron schedule.
pub const CRON_UPDATE: Entry = Entry::new("cron_update", Runtime::Supervisor);
/// Runs a cron schedule now.
pub const CRON_RUN: Entry = Entry::new("cron_run", Runtime::Supervisor);
/// Lists a cron schedule's runs.
pub const CRON_RUNS: Entry = Entry::new("cron_runs", Runtime::Supervisor);
/// A nested loop the model waits on.
pub const DELEGATE: Entry = Entry::new("delegate", Runtime::Supervisor);
/// A nested loop that runs alongside.
pub const SPAWN: Entry = Entry::new("spawn", Runtime::Supervisor);
/// A shell command on a pseudo-terminal.
pub const SHELL: Entry = Entry::new("shell", Runtime::Sandbox);
/// A process the model drives across calls.
pub const EXEC_COMMAND: Entry = Entry::new("exec_command", Runtime::Sandbox);
/// Input to a running process.
pub const WRITE_STDIN: Entry = Entry::new("write_stdin", Runtime::Sandbox);
/// Git on the clone the supervisor made.
pub const GIT: Entry = Entry::new("git", Runtime::Sandbox);
/// Reads a file in `/workspace`.
pub const FILE_READ: Entry = Entry::new("file_read", Runtime::Sandbox);
/// Reads a file with line hashes.
pub const FILE_READ_HASHED: Entry = Entry::new("file_read_hashed", Runtime::Sandbox);
/// Writes a file.
pub const FILE_WRITE: Entry = Entry::new("file_write", Runtime::Sandbox);
/// Appends to a file.
pub const FILE_APPEND: Entry = Entry::new("file_append", Runtime::Sandbox);
/// Deletes a file.
pub const FILE_DELETE: Entry = Entry::new("file_delete", Runtime::Sandbox);
/// Replaces text in a file.
pub const FILE_EDIT: Entry = Entry::new("file_edit", Runtime::Sandbox);
/// Replaces lines named by their hashes.
pub const FILE_EDIT_HASHED: Entry = Entry::new("file_edit_hashed", Runtime::Sandbox);
/// Applies a patch.
pub const APPLY_PATCH: Entry = Entry::new("apply_patch", Runtime::Sandbox);
/// An image file, read through the executor for the next model turn.
pub const IMAGE: Entry = Entry::new("image", Runtime::Sandbox);
/// Chromium, driven over the Chrome `DevTools` Protocol.
pub const BROWSER: Entry = Entry::new("browser", Runtime::Sandbox);
/// Opens a page in Chromium.
pub const BROWSER_OPEN: Entry = Entry::new("browser_open", Runtime::Sandbox);
/// A screenshot of Chromium's page.
pub const SCREENSHOT: Entry = Entry::new("screenshot", Runtime::Sandbox);

/// Every published tool.
pub const PUBLISHED: [&Entry; 35] = [
    &HTTP_REQUEST,
    &WEB_FETCH,
    &PUSHOVER,
    &WEB_SEARCH,
    &MEMORY_STORE,
    &MEMORY_RECALL,
    &MEMORY_LIST,
    &MEMORY_FORGET,
    &UPDATE_PLAN,
    &MESSAGE,
    &SCHEDULE,
    &CRON_ADD,
    &CRON_LIST,
    &CRON_REMOVE,
    &CRON_UPDATE,
    &CRON_RUN,
    &CRON_RUNS,
    &DELEGATE,
    &SPAWN,
    &SHELL,
    &EXEC_COMMAND,
    &WRITE_STDIN,
    &GIT,
    &FILE_READ,
    &FILE_READ_HASHED,
    &FILE_WRITE,
    &FILE_APPEND,
    &FILE_DELETE,
    &FILE_EDIT,
    &FILE_EDIT_HASHED,
    &APPLY_PATCH,
    &IMAGE,
    &BROWSER,
    &BROWSER_OPEN,
    &SCREENSHOT,
];

/// The published entry named `name`, if any.
#[must_use]
pub fn published(name: &str) -> Option<&'static Entry> {
    PUBLISHED.into_iter().find(|entry| entry.name == name)
}

/// The handlers this runner hosts.
#[derive(Debug)]
pub struct Catalog {
    handlers: Vec<Box<dyn Tool>>,
}

impl Catalog {
    /// A catalog hosting `handlers`. Where two serve one tool, the first is used.
    #[must_use]
    pub fn new(handlers: Vec<Box<dyn Tool>>) -> Self {
        Self { handlers }
    }

    /// A catalog hosting every handler this runner carries, its egress tools
    /// sending through `transport`.
    #[must_use]
    pub fn hosted(transport: Arc<dyn Transport>) -> Self {
        Self::new(vec![
            Typed::boxed(HttpRequest::new(Arc::clone(&transport))),
            Typed::boxed(WebFetch::new(Arc::clone(&transport))),
            Typed::boxed(Pushover::new(transport)),
            Typed::boxed(MemoryStore),
            Typed::boxed(MemoryRecall),
            Typed::boxed(MemoryList),
            Typed::boxed(MemoryForget),
            Typed::boxed(UpdatePlan),
        ])
    }

    /// The tools a lease naming `names` is offered.
    ///
    /// # Errors
    /// The first name with no handler here, or no published tool at all: the
    /// lease is refused rather than run without it.
    pub fn select<S: AsRef<str>>(&self, names: &[S]) -> Result<Selection<'_>> {
        let mut selection = Selection::default();
        for name in names.iter().map(AsRef::as_ref) {
            if selection.offers(name) {
                continue;
            }
            let entry = published(name).ok_or_else(|| error::unhosted(name))?;
            match entry.runtime {
                Runtime::Provider => selection.hosted.push(entry),
                Runtime::Supervisor | Runtime::Sandbox => {
                    let handler = self.handler(name).ok_or_else(|| error::unhosted(name))?;
                    selection.tools.push(handler);
                }
            }
        }
        Ok(selection)
    }

    fn handler(&self, name: &str) -> Option<&dyn Tool> {
        self.handlers
            .iter()
            .map(Box::as_ref)
            .find(|handler| handler.name() == name)
    }
}

/// The tools one lease is offered: handlers the router runs, and tools the
/// provider hosts.
#[derive(Debug, Default)]
pub struct Selection<'c> {
    tools: Vec<&'c dyn Tool>,
    hosted: Vec<&'static Entry>,
}

impl<'c> Selection<'c> {
    /// Whether any offered tool runs inside the sandbox; a lease with none
    /// starts no sandbox.
    #[must_use]
    pub fn needs_sandbox(&self) -> bool {
        self.tools
            .iter()
            .any(|tool| tool.runtime() == Runtime::Sandbox)
    }

    /// The handler for `name`, when the lease was offered one.
    #[must_use]
    pub fn tool(&self, name: &str) -> Option<&'c dyn Tool> {
        self.tools.iter().copied().find(|tool| tool.name() == name)
    }

    /// Whether `name` is one of the provider-hosted tools offered.
    #[must_use]
    pub fn hosts(&self, name: &str) -> bool {
        self.hosted.iter().any(|entry| entry.name == name)
    }

    /// The handlers the lease was offered.
    pub fn tools(&self) -> impl Iterator<Item = &'c dyn Tool> + '_ {
        self.tools.iter().copied()
    }

    /// The provider-hosted tools offered.
    #[must_use]
    pub fn hosted(&self) -> &[&'static Entry] {
        &self.hosted
    }

    fn offers(&self, name: &str) -> bool {
        self.tool(name).is_some() || self.hosts(name)
    }
}

#[cfg(test)]
#[path = "catalog/tests.rs"]
mod tests;
