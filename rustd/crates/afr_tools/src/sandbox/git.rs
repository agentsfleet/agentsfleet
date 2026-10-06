//! `git`: the toolbox's git, run on the repository the supervisor checked out.
//!
//! The sandbox has no network, so a subcommand that reaches a remote cannot
//! work here. It is refused with a code before anything runs, naming where a
//! change leaves instead. The refusal is an answer, not the boundary: the
//! boundary is the sandbox's missing network.
//!
//! git runs programs of its own accord: hooks, aliases that start with `!`,
//! and commands named in configuration such as `core.fsmonitor`, set with
//! `-c` or `git config`. So a lease offered `git` can run what a lease
//! offered `shell` can, inside the same sandbox and under the same limits,
//! and nothing here tries to refuse those routes one by one.

use afr_executor::Spawn;
use schemars::JsonSchema;
use serde::Deserialize;

use super::oneshot::{run_to_end, timeout_of};
use super::{executor_of, with_environment};
use crate::catalog::{Entry, GIT};
use crate::handler::Handler;
use crate::runtime::{ToolContext, ToolErrorCode, ToolOutput};

/// The git the toolbox carries, found on the executor's search path.
const GIT_PROGRAM: &str = "git";
/// Subcommands that reach a remote.
pub(crate) const GIT_REFUSED_SUBCOMMANDS: [&str; 5] = ["push", "fetch", "pull", "remote", "clone"];
/// git's global options that take their value as the next argument, so the
/// subcommand is found after them.
const VALUED_OPTIONS: [&str; 7] = [
    "-C",
    "-c",
    "--git-dir",
    "--work-tree",
    "--namespace",
    "--config-env",
    "--attr-source",
];
/// The event a refused subcommand logs under.
const EVENT_REFUSED: &str = "git_subcommand_refused";
/// What a refused subcommand reads back after its name.
const NEEDS_NETWORK: &str =
    "reaches a remote, and this sandbox has no network; a change leaves through propose_change";

/// `git`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Invocation {
    /// The arguments after `git`, one per element, such as
    /// `["log", "--oneline", "-5"]`.
    args: Vec<String>,
    /// How long it may run, in milliseconds: 10000 when absent, at most
    /// 600000.
    #[serde(default)]
    timeout_ms: Option<u64>,
}

/// Runs git in the workspace.
#[derive(Debug)]
pub(crate) struct Git;

#[async_trait::async_trait]
impl Handler for Git {
    const ENTRY: &'static Entry = &GIT;
    const DESCRIPTION: &'static str = "Run git on the repository checked out in the \
        workspace and read back its output and exit code. Local commands only: push, fetch, \
        pull, remote and clone are refused, because the sandbox has no network.";
    type Arguments = Invocation;

    async fn run(&self, arguments: Invocation, context: ToolContext<'_, '_>) -> ToolOutput {
        let lease_id = context.lease.egress.lease_id();
        if let Some(refused) = subcommand(&arguments.args)
            .filter(|subcommand| GIT_REFUSED_SUBCOMMANDS.contains(subcommand))
        {
            let event = EVENT_REFUSED;
            let subcommand = refused;
            tracing::info!(lease_id, subcommand, event);
            return ToolOutput::failed(
                ToolErrorCode::SubcommandNotAllowed,
                &format!("git {refused} {NEEDS_NETWORK}"),
            );
        }
        let executor = match executor_of(&context) {
            Ok(executor) => executor,
            Err(refused) => return refused,
        };
        let spawn = with_environment(Spawn::program(GIT_PROGRAM).args(&arguments.args));
        // In the one repository the lease holds; with several, the model names
        // one with `-C`.
        let spawn = match context.lease.checkouts.as_slice() {
            [only] => spawn.cwd(only.name),
            _none_or_several => spawn,
        };
        run_to_end(executor, spawn, timeout_of(arguments.timeout_ms), lease_id).await
    }
}

/// The subcommand `args` runs: the first argument that is neither one of git's
/// global options nor the value one of them takes.
fn subcommand(args: &[String]) -> Option<&str> {
    let mut rest = args.iter().map(String::as_str);
    while let Some(arg) = rest.next() {
        if VALUED_OPTIONS.contains(&arg) {
            rest.next();
        } else if !arg.starts_with('-') {
            return Some(arg);
        }
    }
    None
}

#[cfg(test)]
#[path = "git/tests.rs"]
mod tests;
