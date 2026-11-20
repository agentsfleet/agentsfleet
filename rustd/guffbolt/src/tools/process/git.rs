//! `git`: the environment's git, run on the repository the embedder checked out.
//!
//! The repository's remote is the runner's: it checked the repository out on
//! the host, and the token that fetched it never enters the sandbox
//! (`afr_supervisor`'s workspace clone). So a subcommand that reaches a remote
//! cannot work here, and is refused with a code before anything runs, naming
//! where a change leaves instead. The refusal is an answer, not the boundary:
//! the boundary is the missing credential, and what the sandbox's network
//! reaches is its egress policy's to say.
//!
//! git runs programs of its own accord: hooks, aliases that start with `!`,
//! and commands named in configuration such as `core.fsmonitor`, set with
//! `-c` or `git config`. So a run offered `git` can run what a run
//! offered `shell` can, inside the same sandbox and under the same limits,
//! and nothing here tries to refuse those routes one by one.

use schemars::JsonSchema;
use serde::Deserialize;

use super::oneshot::{run_to_end, timeout_of};
use super::{env_of, with_environment};
use crate::tool::ToolOutput;
use crate::tools::env::Spawn;
use crate::tools::{Context, Handler, code};

/// The git the environment carries, found on its search path.
const GIT_PROGRAM: &str = "git";
/// Subcommands that reach a remote.
pub const GIT_REFUSED_SUBCOMMANDS: [&str; 5] = ["push", "fetch", "pull", "remote", "clone"];
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
pub const EVENT_REFUSED: &str = "git_subcommand_refused";
/// What a refused subcommand reads back after its name.
const REACHES_REMOTE: &str =
    "reaches a remote, which only the runner reaches; a change leaves through propose_change";

/// `git`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Invocation {
    /// The arguments after `git`, one per element, such as
    /// `["log", "--oneline", "-5"]`.
    args: Vec<String>,
    /// How long it may run, in milliseconds: 10000 when absent, at most
    /// 600000.
    #[serde(default)]
    timeout_ms: Option<u64>,
}

/// Runs git in the workspace.
#[derive(Debug, Clone, Copy)]
pub struct Git;

impl Handler for Git {
    const NAME: &'static str = "git";
    const DESCRIPTION: &'static str = "Run git on the repository checked out in the \
        workspace and read back its output and exit code. Local commands only: push, fetch, \
        pull, remote and clone are refused, because only the runner reaches the repository's \
        remote; a change leaves through propose_change.";
    type Arguments = Invocation;

    async fn run(&self, arguments: Invocation, context: Context<'_>) -> ToolOutput {
        let run_id = context.run_id;
        if let Some(refused) = subcommand(&arguments.args)
            .filter(|subcommand| GIT_REFUSED_SUBCOMMANDS.contains(subcommand))
        {
            let event = EVENT_REFUSED;
            let subcommand = refused;
            tracing::info!(run_id, subcommand, event);
            return ToolOutput::failed(
                code::SUBCOMMAND_NOT_ALLOWED,
                &format!("git {refused} {REACHES_REMOTE}"),
            );
        }
        let env = match env_of(&context) {
            Ok(env) => env,
            Err(refused) => return refused,
        };
        let spawn = with_environment(Spawn::program(GIT_PROGRAM).args(&arguments.args));
        // In the one repository the run holds; with several, the model names
        // one with `-C`.
        let spawn = match context.repository {
            Some(repository) => spawn.cwd(repository),
            None => spawn,
        };
        run_to_end(env, spawn, timeout_of(arguments.timeout_ms), run_id).await
    }
}

/// The subcommand `args` runs: the first argument that is neither one of git's
/// global options nor the value one of them takes.
pub fn subcommand(args: &[String]) -> Option<&str> {
    let mut rest = args.iter().map(String::as_str);
    while let Some(arg) = rest.next() {
        if VALUED_OPTIONS.contains(&arg) {
            rest.next();
            continue;
        }
        if !arg.starts_with('-') {
            return Some(arg);
        }
    }
    None
}
