//! The tools every run is offered: the runner's published catalog and its
//! sub-agent tools, spelled once for the daemon.
//!
//! The daemon never depends on the runner, so the names are spelled here, in
//! the runner's catalog order (`afr_tools/src/catalog/mod.rs`, `PUBLISHED`).
//! The runner refuses a run naming a tool it does not host, so a name that
//! drifts fails the run loudly rather than shrinking what it may call.

use std::borrow::Cow;

/// Every published tool but the two task tools.
pub const PUBLISHED: [&str; 36] = [
    "http_request",
    "web_fetch",
    "notify",
    "web_search",
    "memory_store",
    "memory_recall",
    "memory_list",
    "memory_forget",
    "update_plan",
    "ask_user",
    "message",
    "fleet_steer",
    "fleet_activity",
    "schedule",
    "cron_add",
    "cron_list",
    "cron_remove",
    "cron_update",
    "cron_run",
    "cron_runs",
    "trigger_add",
    "trigger_list",
    "trigger_remove",
    "request_access",
    "shell",
    "exec_command",
    "write_stdin",
    "git",
    "file_read",
    "file_write",
    "file_edit",
    "apply_patch",
    "image",
    "browser",
    "browser_open",
    "screenshot",
];

/// The sub-agent tools the runner publishes under `guffbolt`'s own names
/// (`guffbolt::agents::TOOLS`).
pub const SUB_AGENTS: [&str; 7] = [
    "spawn_agent",
    "send_input",
    "followup_task",
    "wait_agent",
    "list_agents",
    "close_agent",
    "interrupt_agent",
];

/// The tools that start a durable task and join a message to one: offered
/// only to a run that can be judged, because a task's runs end with the
/// decision model deciding whether it is done.
pub const TASK_TOOLS: [&str; 2] = ["task_start", "task_input"];

/// The tools a run is offered: every published tool and every sub-agent tool,
/// and the task tools when `judged`.
#[must_use]
pub fn offered<'a>(judged: bool) -> Vec<Cow<'a, str>> {
    let tasks: &[&'static str] = if judged { &TASK_TOOLS } else { &[] };
    PUBLISHED
        .iter()
        .chain(&SUB_AGENTS)
        .chain(tasks)
        .map(|&name| Cow::Borrowed(name))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{PUBLISHED, SUB_AGENTS, TASK_TOOLS, offered};

    #[test]
    fn a_judged_run_is_offered_every_tool_and_the_task_tools() {
        let tools = offered(true);
        assert_eq!(tools.len(), 45);
        for name in PUBLISHED.iter().chain(&SUB_AGENTS).chain(&TASK_TOOLS) {
            assert!(tools.iter().any(|tool| tool == name), "{name} is offered");
        }
    }

    #[test]
    fn a_run_that_cannot_be_judged_is_offered_no_task_tool() {
        let tools = offered(false);
        assert_eq!(tools.len(), 43);
        for name in TASK_TOOLS {
            assert!(!tools.iter().any(|tool| tool == name), "{name} is withheld");
        }
        assert!(tools.iter().any(|tool| tool == "request_access"));
        assert!(tools.iter().any(|tool| tool == "spawn_agent"));
    }

    #[test]
    fn no_tool_is_offered_twice() {
        let tools = offered(true);
        for (index, tool) in tools.iter().enumerate() {
            assert!(
                !tools.iter().take(index).any(|earlier| earlier == tool),
                "{tool} is offered twice"
            );
        }
    }
}
