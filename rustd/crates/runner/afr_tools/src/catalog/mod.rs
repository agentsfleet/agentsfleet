//! The catalog: every published tool and where it runs, the handlers this
//! runner hosts, and the subset one run is offered.
//!
//! The published list is `docs/architecture/runner_execution.md` §"Tool
//! catalog", one entry per tool. A run's `ExecutionPolicy.tools` selects from
//! it; a name with no handler here refuses the run, never a quieter tool set.
//! The tools a run builds over its own backends ([`BACKED`]) need none: the
//! run builds them, `guffbolt`'s, when it starts ([`crate::Backends`]). Nor
//! do `guffbolt`'s subagent tools, which this catalog publishes under
//! `guffbolt`'s own names rather than its own list ([`builds`]).

use std::sync::Arc;

use af_egress::Transport;
use guffbolt::tools::{Backend, built_in};

use crate::egress::SharedTransport;
use crate::error::{self, Result};
use crate::handler::Typed;
use crate::runtime::{CatalogTool, Runtime};
use crate::selection::Offered;
use crate::selection::Selection;
use crate::verbs::{FleetActivity, FleetSteer};

/// One published tool: its name and the runtime it executes in.
///
/// Built only here, so every entry a selection holds names a published tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
/// A push notification to the person the fleet works for, through Pushover.
pub const NOTIFY: Entry = Entry::new("notify", Runtime::Supervisor);
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
/// Work this fleet puts on another fleet in its workspace, through a runner
/// verb.
pub const FLEET_STEER: Entry = Entry::new("fleet_steer", Runtime::Supervisor);
/// The workspace's fleets and one fleet's recent events, through a runner verb.
pub const FLEET_ACTIVITY: Entry = Entry::new("fleet_activity", Runtime::Supervisor);
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
/// One shell command, run to its end.
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
    &NOTIFY,
    &WEB_SEARCH,
    &MEMORY_STORE,
    &MEMORY_RECALL,
    &MEMORY_LIST,
    &MEMORY_FORGET,
    &UPDATE_PLAN,
    &MESSAGE,
    &FLEET_STEER,
    &FLEET_ACTIVITY,
    &SCHEDULE,
    &CRON_ADD,
    &CRON_LIST,
    &CRON_REMOVE,
    &CRON_UPDATE,
    &CRON_RUN,
    &CRON_RUNS,
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

/// Every published tool a run builds over its own backends when it starts:
/// every tool of `guffbolt`'s this catalog publishes. The provider's
/// `web_search` and this runner's own fleet tools are the rest.
pub const BACKED: [&Entry; 32] = [
    &HTTP_REQUEST,
    &WEB_FETCH,
    &NOTIFY,
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

/// How many subagent tools `guffbolt` builds, which a run names as it names
/// a published tool.
pub const CHILDREN: usize = guffbolt::agents::TOOLS.len();

/// `guffbolt`'s subagent tool named `name`, when it is one.
///
/// It runs over the run's children, beside the loop, and this catalog
/// publishes it under `guffbolt`'s own name, never spelled here.
#[must_use]
pub fn children_tool(name: &str) -> Option<&'static str> {
    built_in(name)
        .filter(|tool| tool.needs == [Backend::Agents])
        .map(|tool| tool.name)
}

/// Whether a run builds the tool named `name` over its own backends: a
/// [`BACKED`] entry, or one of `guffbolt`'s subagent tools.
#[must_use]
pub fn builds(name: &str) -> bool {
    BACKED.iter().any(|entry| entry.name == name) || children_tool(name).is_some()
}

/// The published entry named `name`, if any.
#[must_use]
pub fn published(name: &str) -> Option<&'static Entry> {
    PUBLISHED.into_iter().find(|entry| entry.name == name)
}

/// The handlers this runner hosts, and where a run's own notifier sends.
#[derive(Debug)]
pub struct Catalog {
    handlers: Vec<Box<CatalogTool>>,
    transport: Option<SharedTransport>,
}

impl Catalog {
    /// A catalog hosting `handlers`, whose runs send no notification. Where
    /// two serve one tool, the first is used.
    #[must_use]
    pub fn new(handlers: Vec<Box<CatalogTool>>) -> Self {
        Self {
            handlers,
            transport: None,
        }
    }

    /// A catalog hosting every handler this runner carries: the fleet tools
    /// over its verbs. Each run's web tools and notifier send through
    /// `transport`; every other tool is `guffbolt`'s, which the run builds.
    #[must_use]
    pub fn hosted(transport: Arc<dyn Transport>) -> Self {
        let mut hosted = Self::new(vec![Typed::boxed(FleetSteer), Typed::boxed(FleetActivity)]);
        hosted.transport = Some(transport);
        hosted
    }

    /// Where each run's web tools and notifier send; `None` for a catalog
    /// that sends none, whose requests are refused.
    #[must_use]
    pub fn transport(&self) -> Option<&dyn Transport> {
        self.transport.as_deref()
    }

    /// The tools a run naming `names` is offered: a handler this catalog
    /// hosts, else a tool the run builds over its own backends, else one of
    /// `guffbolt`'s subagent tools.
    ///
    /// # Errors
    /// The first name with none of these, or no published tool at all: the
    /// run is refused rather than run without it.
    pub fn select<S: AsRef<str>>(&self, names: &[S]) -> Result<Selection<'_>> {
        let mut selection = Selection::default();
        for name in names.iter().map(AsRef::as_ref) {
            if selection.offers(name) {
                continue;
            }
            let Some(entry) = published(name) else {
                let built = children_tool(name).ok_or_else(|| error::unhosted(name))?;
                selection.tools.push(Offered::Built(built));
                continue;
            };
            if entry.runtime == Runtime::Provider {
                selection.hosted.push(entry);
                continue;
            }
            let offered = match self.handler(name) {
                Some(handler) => Offered::Handler(entry, handler),
                None if BACKED.contains(&entry) => Offered::Backed(entry),
                None => return Err(error::unhosted(name)),
            };
            selection.tools.push(offered);
        }
        Ok(selection)
    }

    fn handler(&self, name: &str) -> Option<&CatalogTool> {
        self.handlers
            .iter()
            .map(Box::as_ref)
            .find(|handler| handler.name() == name)
    }
}

#[cfg(test)]
mod tests;
