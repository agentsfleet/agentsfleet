//! Every tool guffbolt builds, with the backends it needs, in the order `*`
//! offers them.

use crate::tools::agents::{
    CloseAgent, FollowupTask, InterruptAgent, ListAgents, SendInput, SpawnAgent, WaitAgent,
};
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
use crate::tools::{ContextTool, Handler, Typed};

use super::{Backend, BuiltIn, Given};

/// The environment alone.
const ENV: &[Backend] = &[Backend::Env];
/// The environment and the sessions kept across calls.
const SESSIONS: &[Backend] = &[Backend::Env, Backend::Sessions];

/// `name`, built over `needs`.
const fn over(name: &'static str, needs: &'static [Backend]) -> BuiltIn {
    BuiltIn { name, needs }
}

/// Every built-in tool, in the order `*` offers them.
pub(super) const TOOLS: [BuiltIn; 45] = [
    over("shell", ENV),
    over("exec_command", SESSIONS),
    over("write_stdin", SESSIONS),
    over("git", ENV),
    over("file_read", ENV),
    over("file_read_hashed", ENV),
    over("file_write", ENV),
    over("file_append", ENV),
    over("file_delete", ENV),
    over("file_edit", ENV),
    over("file_edit_hashed", ENV),
    over("apply_patch", ENV),
    over("image", ENV),
    over("ls", ENV),
    over("find", ENV),
    over("grep", ENV),
    over("browser_open", ENV),
    over("browser", ENV),
    over("screenshot", ENV),
    over("http_request", &[Backend::Outbound]),
    over("web_fetch", &[Backend::Outbound]),
    over("memory_store", &[Backend::Memory]),
    over("memory_recall", &[Backend::Memory]),
    over("memory_list", &[Backend::Memory]),
    over("memory_forget", &[Backend::Memory]),
    over("cron_add", &[Backend::Scheduler]),
    over("cron_list", &[Backend::Scheduler]),
    over("cron_update", &[Backend::Scheduler]),
    over("cron_remove", &[Backend::Scheduler]),
    over("cron_run", &[Backend::Scheduler]),
    over("cron_runs", &[Backend::Scheduler]),
    over("schedule", &[Backend::Scheduler]),
    over("message", &[Backend::Channel]),
    over("notify", &[Backend::Notifier]),
    over("spawn_agent", &[Backend::Agents]),
    over("send_input", &[Backend::Agents]),
    over("followup_task", &[Backend::Agents]),
    over("wait_agent", &[Backend::Agents]),
    over("list_agents", &[Backend::Agents]),
    over("close_agent", &[Backend::Agents]),
    over("interrupt_agent", &[Backend::Agents]),
    over("update_plan", &[]),
    over("ask_user", &[]),
    over("current_time", &[]),
    over("sleep", &[]),
];

/// `handler`, boxed as a loop holds it.
fn boxed<'run, H: Handler + 'run>(handler: H) -> Box<ContextTool<'run>> {
    Box::new(Typed::new(handler))
}

/// The built-in tool named `name`, over what `given` holds; `None` for a
/// name no built-in has, or one whose backend was not given.
pub(super) fn tool<'run>(given: Given<'run>, name: &str) -> Option<Box<ContextTool<'run>>> {
    plain_tool(name)
        .or_else(|| env_tool(name, given))
        .or_else(|| file_tool(name, given.env?))
        .or_else(|| backed_tool(name, given))
        .or_else(|| agent_tool(name, given.agents?))
}

/// The tool named `name` that runs a process or drives the browser in the
/// environment.
fn env_tool<'run>(name: &str, given: Given<'run>) -> Option<Box<ContextTool<'run>>> {
    let env = given.env?;
    Some(match name {
        "shell" => boxed(Shell::new(env)),
        "exec_command" => boxed(ExecCommand::new(env, given.sessions?)),
        "write_stdin" => boxed(WriteStdin::new(env, given.sessions?)),
        "git" => boxed(Git::new(env, given.repository)),
        "image" => boxed(ImageTool::new(env, given.image_input)),
        "browser_open" => boxed(BrowserOpen::new(env)),
        "browser" => boxed(Browse::new(env)),
        "screenshot" => boxed(Screenshot::new(env)),
        _other => return None,
    })
}

/// The tool named `name` that reads or changes the environment's files.
fn file_tool<'run>(name: &str, env: &'run dyn crate::tools::Env) -> Option<Box<ContextTool<'run>>> {
    Some(match name {
        "file_read" => boxed(FileRead::new(env)),
        "file_read_hashed" => boxed(FileReadHashed::new(env)),
        "file_write" => boxed(FileWrite::new(env)),
        "file_append" => boxed(FileAppend::new(env)),
        "file_delete" => boxed(FileDelete::new(env)),
        "file_edit" => boxed(FileEdit::new(env)),
        "file_edit_hashed" => boxed(FileEditHashed::new(env)),
        "apply_patch" => boxed(ApplyPatch::new(env)),
        "ls" => boxed(Ls::new(env)),
        "find" => boxed(Find::new(env)),
        "grep" => boxed(Grep::new(env)),
        _other => return None,
    })
}

/// The tool named `name` over the network, memory, a scheduler, a channel or
/// a notifier.
fn backed_tool<'run>(name: &str, given: Given<'run>) -> Option<Box<ContextTool<'run>>> {
    Some(match name {
        "http_request" => boxed(HttpRequest::new(given.outbound?)),
        "web_fetch" => boxed(WebFetch::new(given.outbound?)),
        "memory_store" => boxed(MemoryStoreTool::new(given.memory?)),
        "memory_recall" => boxed(MemoryRecall::new(given.memory?)),
        "memory_list" => boxed(MemoryList::new(given.memory?)),
        "memory_forget" => boxed(MemoryForget::new(given.memory?)),
        "cron_add" => boxed(CronAdd::new(given.scheduler?)),
        "cron_list" => boxed(CronList::new(given.scheduler?)),
        "cron_update" => boxed(CronUpdate::new(given.scheduler?)),
        "cron_remove" => boxed(CronRemove::new(given.scheduler?)),
        "cron_run" => boxed(CronRun::new(given.scheduler?)),
        "cron_runs" => boxed(CronRuns::new(given.scheduler?)),
        "schedule" => boxed(ScheduleOnce::new(given.scheduler?)),
        "message" => boxed(Message::new(given.channel?)),
        "notify" => boxed(NotifyTool::new(given.notifier?)),
        _other => return None,
    })
}

/// The subagent tool named `name`, over the run's children.
fn agent_tool<'run>(
    name: &str,
    agents: &'run crate::agents::Agents,
) -> Option<Box<ContextTool<'run>>> {
    Some(match name {
        "spawn_agent" => boxed(SpawnAgent::new(agents)),
        "send_input" => boxed(SendInput::new(agents)),
        "followup_task" => boxed(FollowupTask::new(agents)),
        "wait_agent" => boxed(WaitAgent::new(agents)),
        "list_agents" => boxed(ListAgents::new(agents)),
        "close_agent" => boxed(CloseAgent::new(agents)),
        "interrupt_agent" => boxed(InterruptAgent::new(agents)),
        _other => return None,
    })
}

/// The tool named `name` that takes no backend.
fn plain_tool<'run>(name: &str) -> Option<Box<ContextTool<'run>>> {
    Some(match name {
        "update_plan" => boxed(UpdatePlan),
        "ask_user" => boxed(AskUser),
        "current_time" => boxed(CurrentTime),
        "sleep" => boxed(Sleep),
        _other => return None,
    })
}
