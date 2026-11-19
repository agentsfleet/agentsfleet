//! The catalog: every published tool and where it runs, the handlers this
//! runner hosts, and the subset one run is offered.
//!
//! The published list is `docs/architecture/runner_execution.md` §"Tool
//! catalog", one entry per tool. A run's `ExecutionPolicy.tools` selects from
//! it; a name with no handler here refuses the run, never a quieter tool set.
//! The nested tools need none: the loop runs them itself.

use std::sync::Arc;

use af_egress::Transport;

use guffbolt::tools::browser::{Browse, BrowserOpen, Screenshot};
use guffbolt::tools::files::{
    ApplyPatch, FileAppend, FileDelete, FileEdit, FileEditHashed, FileRead, FileReadHashed,
    FileWrite, ImageTool,
};
use guffbolt::tools::memory::{MemoryForget, MemoryList, MemoryRecall, MemoryStoreTool};
use guffbolt::tools::plan::UpdatePlan;
use guffbolt::tools::process::{ExecCommand, Git, Shell, WriteStdin};
use guffbolt::tools::web::{HttpRequest, Pushover, WebFetch};

use crate::error::{self, Result};
use crate::generic::Generic;
use crate::handler::Typed;
use crate::nested::Nested;
use crate::runtime::{CatalogTool, Runtime};
use crate::selection::Offered;
use crate::selection::Selection;
use crate::verbs::{
    CronAdd, CronList, CronRemove, CronRun, CronRuns, CronUpdate, FleetActivity, FleetSteer,
    Message, ScheduleOnce,
};

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
/// A nested loop the model waits on.
pub const DELEGATE: Entry = Entry::new("delegate", Runtime::Supervisor);
/// A nested loop that runs alongside.
pub const SPAWN: Entry = Entry::new("spawn", Runtime::Supervisor);
/// A spawned loop's answer, or that it still runs.
pub const WAIT_AGENT: Entry = Entry::new("wait_agent", Runtime::Supervisor);
/// A message for a spawned loop's next turn.
pub const SEND_INPUT: Entry = Entry::new("send_input", Runtime::Supervisor);
/// Every child of the run and its state.
pub const LIST_AGENTS: Entry = Entry::new("list_agents", Runtime::Supervisor);
/// Ends one spawned loop.
pub const INTERRUPT_AGENT: Entry = Entry::new("interrupt_agent", Runtime::Supervisor);
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
pub const PUBLISHED: [&Entry; 41] = [
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
    &FLEET_STEER,
    &FLEET_ACTIVITY,
    &SCHEDULE,
    &CRON_ADD,
    &CRON_LIST,
    &CRON_REMOVE,
    &CRON_UPDATE,
    &CRON_RUN,
    &CRON_RUNS,
    &DELEGATE,
    &SPAWN,
    &WAIT_AGENT,
    &SEND_INPUT,
    &LIST_AGENTS,
    &INTERRUPT_AGENT,
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
    handlers: Vec<Box<CatalogTool>>,
}

impl Catalog {
    /// A catalog hosting `handlers`. Where two serve one tool, the first is used.
    #[must_use]
    pub fn new(handlers: Vec<Box<CatalogTool>>) -> Self {
        Self { handlers }
    }

    /// A catalog hosting every handler this runner carries, its egress tools
    /// sending through `transport`. The generic ones are `guffbolt`'s, each
    /// lent the run's sandbox, guard and memory per call.
    #[must_use]
    pub fn hosted(transport: Arc<dyn Transport>) -> Self {
        Self::new(vec![
            Generic::sending(HttpRequest, Arc::clone(&transport)),
            Generic::sending(WebFetch, Arc::clone(&transport)),
            Generic::sending(Pushover, transport),
            Generic::boxed(MemoryStoreTool),
            Generic::boxed(MemoryRecall),
            Generic::boxed(MemoryList),
            Generic::boxed(MemoryForget),
            Generic::boxed(UpdatePlan),
            Generic::boxed(Shell),
            Generic::boxed(ExecCommand),
            Generic::boxed(WriteStdin),
            Generic::boxed(Git),
            Generic::boxed(FileRead),
            Generic::boxed(FileReadHashed),
            Generic::boxed(FileWrite),
            Generic::boxed(FileAppend),
            Generic::boxed(FileDelete),
            Generic::boxed(FileEdit),
            Generic::boxed(FileEditHashed),
            Generic::boxed(ApplyPatch),
            Generic::boxed(ImageTool),
            Generic::boxed(BrowserOpen),
            Generic::boxed(Browse),
            Generic::boxed(Screenshot),
            Typed::boxed(Message),
            Typed::boxed(FleetSteer),
            Typed::boxed(FleetActivity),
            Typed::boxed(ScheduleOnce),
            Typed::boxed(CronAdd),
            Typed::boxed(CronList),
            Typed::boxed(CronRemove),
            Typed::boxed(CronUpdate),
            Typed::boxed(CronRun),
            Typed::boxed(CronRuns),
        ])
    }

    /// The tools a run naming `names` is offered.
    ///
    /// # Errors
    /// The first name with no handler here, or no published tool at all: the
    /// run is refused rather than run without it.
    pub fn select<S: AsRef<str>>(&self, names: &[S]) -> Result<Selection<'_>> {
        let mut selection = Selection::default();
        for name in names.iter().map(AsRef::as_ref) {
            if selection.offers(name) {
                continue;
            }
            let entry = published(name).ok_or_else(|| error::unhosted(name))?;
            match (entry.runtime, Nested::of(name)) {
                (Runtime::Provider, _) => selection.hosted.push(entry),
                (Runtime::Supervisor | Runtime::Sandbox, Some(nested)) => {
                    selection.tools.push(Offered::Nested(nested));
                }
                (Runtime::Supervisor | Runtime::Sandbox, None) => {
                    let handler = self.handler(name).ok_or_else(|| error::unhosted(name))?;
                    selection.tools.push(Offered::Handler(entry, handler));
                }
            }
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
