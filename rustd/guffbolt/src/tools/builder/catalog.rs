//! Every tool guffbolt builds, by the backend it needs, in the order `*`
//! offers them.

use crate::tools::ask::AskUser;
use crate::tools::browser::{Browse, BrowserOpen, Screenshot};
use crate::tools::clock::{CurrentTime, Sleep};
use crate::tools::files::{
    ApplyPatch, FileAppend, FileDelete, FileEdit, FileEditHashed, FileRead, FileReadHashed,
    FileWrite, Find, Grep, ImageTool, Ls,
};
use crate::tools::memory::{MemoryForget, MemoryList, MemoryRecall, MemoryStoreTool};
use crate::tools::message::Message;
use crate::tools::notify::NotifyTool;
use crate::tools::plan::UpdatePlan;
use crate::tools::process::{ExecCommand, Git, Shell, WriteStdin};
use crate::tools::schedule::{
    CronAdd, CronList, CronRemove, CronRun, CronRuns, CronUpdate, ScheduleOnce,
};
use crate::tools::web::{HttpRequest, WebFetch};
use crate::tools::{
    Channel, ContextTool, Env, Handler, MemoryStore, Notifier, Outbound, Scheduler, Typed,
};

use super::Backend;
use super::seamed::Seamed;

/// One family of built-in tools: the backend they need and their names.
pub(super) struct Family {
    /// The backend every tool of the family takes; `None` for those that
    /// take none.
    pub(super) needs: Option<Backend>,
    /// Their names, in the order `*` offers them.
    pub(super) names: &'static [&'static str],
}

/// Every built-in tool, family by family.
pub(super) const FAMILIES: [Family; 7] = [
    Family {
        needs: Some(Backend::Env),
        names: &[
            "shell",
            "exec_command",
            "write_stdin",
            "git",
            "file_read",
            "file_read_hashed",
            "file_write",
            "file_append",
            "file_delete",
            "file_edit",
            "file_edit_hashed",
            "apply_patch",
            "image",
            "ls",
            "find",
            "grep",
            "browser_open",
            "browser",
            "screenshot",
        ],
    },
    Family {
        needs: Some(Backend::Outbound),
        names: &["http_request", "web_fetch"],
    },
    Family {
        needs: Some(Backend::Memory),
        names: &[
            "memory_store",
            "memory_recall",
            "memory_list",
            "memory_forget",
        ],
    },
    Family {
        needs: Some(Backend::Scheduler),
        names: &[
            "cron_add",
            "cron_list",
            "cron_update",
            "cron_remove",
            "cron_run",
            "cron_runs",
            "schedule",
        ],
    },
    Family {
        needs: Some(Backend::Channel),
        names: &["message"],
    },
    Family {
        needs: Some(Backend::Notifier),
        names: &["notify"],
    },
    Family {
        needs: None,
        names: &["update_plan", "ask_user", "current_time", "sleep"],
    },
];

/// `handler`, boxed as a loop holds it.
fn boxed<'run, H: Handler + 'run>(handler: H) -> Box<ContextTool<'run>> {
    Box::new(Typed::new(handler))
}

/// The tool named `name` that runs in `env`; `None` for a name no
/// environment tool has.
pub(super) fn env_tool<'run>(name: &str, env: &'run dyn Env) -> Option<Box<ContextTool<'run>>> {
    let tool: Box<ContextTool<'run>> = match name {
        "shell" => boxed(Shell),
        "exec_command" => boxed(ExecCommand),
        "write_stdin" => boxed(WriteStdin),
        "git" => boxed(Git),
        "file_read" => boxed(FileRead),
        "file_read_hashed" => boxed(FileReadHashed),
        "file_write" => boxed(FileWrite),
        "file_append" => boxed(FileAppend),
        "file_delete" => boxed(FileDelete),
        "file_edit" => boxed(FileEdit),
        "file_edit_hashed" => boxed(FileEditHashed),
        "apply_patch" => boxed(ApplyPatch),
        "image" => boxed(ImageTool),
        "ls" => boxed(Ls),
        "find" => boxed(Find),
        "grep" => boxed(Grep),
        "browser_open" => boxed(BrowserOpen),
        "browser" => boxed(Browse),
        "screenshot" => boxed(Screenshot),
        _other => return None,
    };
    Some(Box::new(Seamed::over_env(env, tool)))
}

/// The tool named `name` that sends through `outbound`.
pub(super) fn outbound_tool<'run>(
    name: &str,
    outbound: &'run dyn Outbound,
) -> Option<Box<ContextTool<'run>>> {
    let tool: Box<ContextTool<'run>> = match name {
        "http_request" => boxed(HttpRequest),
        "web_fetch" => boxed(WebFetch),
        _other => return None,
    };
    Some(Box::new(Seamed::over_outbound(outbound, tool)))
}

/// The memory tool named `name`, over `store`.
pub(super) fn memory_tool<'run>(
    name: &str,
    store: &'run dyn MemoryStore,
) -> Option<Box<ContextTool<'run>>> {
    Some(match name {
        "memory_store" => boxed(MemoryStoreTool::new(store)),
        "memory_recall" => boxed(MemoryRecall::new(store)),
        "memory_list" => boxed(MemoryList::new(store)),
        "memory_forget" => boxed(MemoryForget::new(store)),
        _other => return None,
    })
}

/// The schedule tool named `name`, over `scheduler`.
pub(super) fn schedule_tool<'run>(
    name: &str,
    scheduler: &'run dyn Scheduler,
) -> Option<Box<ContextTool<'run>>> {
    Some(match name {
        "cron_add" => boxed(CronAdd::new(scheduler)),
        "cron_list" => boxed(CronList::new(scheduler)),
        "cron_update" => boxed(CronUpdate::new(scheduler)),
        "cron_remove" => boxed(CronRemove::new(scheduler)),
        "cron_run" => boxed(CronRun::new(scheduler)),
        "cron_runs" => boxed(CronRuns::new(scheduler)),
        "schedule" => boxed(ScheduleOnce::new(scheduler)),
        _other => return None,
    })
}

/// `message`, over `channel`.
pub(super) fn channel_tool<'run>(
    name: &str,
    channel: &'run dyn Channel,
) -> Option<Box<ContextTool<'run>>> {
    (name == "message").then(|| boxed(Message::new(channel)))
}

/// `notify`, over `notifier`.
pub(super) fn notifier_tool<'run>(
    name: &str,
    notifier: &'run dyn Notifier,
) -> Option<Box<ContextTool<'run>>> {
    (name == "notify").then(|| boxed(NotifyTool::new(notifier)))
}

/// The tool named `name` that takes no backend.
pub(super) fn plain_tool<'run>(name: &str) -> Option<Box<ContextTool<'run>>> {
    Some(match name {
        "update_plan" => boxed(UpdatePlan),
        "ask_user" => boxed(AskUser),
        "current_time" => boxed(CurrentTime),
        "sleep" => boxed(Sleep),
        _other => return None,
    })
}
