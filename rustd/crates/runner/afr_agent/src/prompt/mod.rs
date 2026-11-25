//! The prompt: the fixed text naming the fleet, then its pinned memory under
//! one heading, the trusted repair context on a write-bound run, where the
//! workspace's repositories are checked out, and, when the run is offered the
//! task tools, the message they name and the fleet's unfinished tasks. The
//! event's message is the latest user turn; the fleet's earlier turns open the
//! conversation ahead of it (`crate::harness::construction`).
//!
//! Pinned memory is every `core` entry of the fleet's own that the run's
//! memory load carried. Other categories and other fleets' shared entries stay
//! recall-only. The load's own byte budget bounds the block.
//!
//! The message is `af_wire::turn::message_of`'s reading of the request, the
//! same reading the daemon gave each earlier turn's message.

use std::fmt;

use af_wire::durable_task::DurableTaskInputCandidate;
use af_wire::memory::{LOAD_WINDOW_BYTES, MemoryDelta, PINNED_CATEGORY};
use af_wire::policy::repository::{self, FIELD_REF, REFS_HEADS, REFS_PATH};
use af_wire::policy::{ExecutionPolicy, HttpMethod, RepositoryAccess};
use af_wire::run::RunPayload;
use af_wire::turn::message_of;
use afr_memory::Seed;
use afr_tools::catalog::is_task_tool;
use afr_tools::sandbox::{Checkout, checkouts};

/// The heading the fleet's pinned memory renders under.
const PINNED_MEMORY: &str = "## Pinned memory";
/// The heading the trusted repair context renders under.
const TRUSTED_REPAIR_CONTEXT: &str = "## Trusted repair context";
/// The heading the workspace's checkouts render under.
const WORKSPACE: &str = "## Workspace";
/// The heading the task tools' source message and unfinished tasks render
/// under.
const DURABLE_TASKS: &str = "## Durable tasks";
/// The branch a checkout is on when its binding names no base: a read
/// binding's, which the supervisor checks out at the remote's default.
const DEFAULT_BRANCH: &str = "its default branch";
/// What separates two blocks of the system prompt.
const BLOCK_BREAK: &str = "\n\n";

/// The text every run's system prompt opens with, the fleet's name filled in.
fn fixed(name: &str) -> String {
    format!(
        "You are {name}, a fleet in agentsfleet: a long-lived agent a team talks to in one chat.\n\
         Remember standing guidance with memory_store under the category core, only when the \
         person's newest message asks you to. It is shown to you at the start of every run. \
         Store anything else under another category.\n\
         Schedule yourself with the cron tools, add a webhook, GitHub or Slack trigger with \
         trigger_add, and start work that must keep going with task_start, each only when the \
         person's newest message asks.\n\
         You hold only the secrets and integrations a person approved for you. When you need \
         one, call request_access with its name (and, for GitHub, the repositories and read or \
         write); the person approves it once."
    )
}

/// What a run asks the model first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Prompt {
    /// The fixed text naming the fleet: all a saved compaction's digest
    /// covers, so a change to pinned memory or access keeps the summary.
    pub(crate) fixed: String,
    /// The system prompt.
    pub(crate) instructions: String,
    /// The latest user turn.
    pub(crate) message: String,
}

impl Prompt {
    /// The prompt for `run`, whose memory load is `memory`: its window, the
    /// fleet's own entries newest first, is where the pinned ones are read.
    pub(crate) fn new(run: &RunPayload<'_>, memory: &Seed<'_>) -> Self {
        let message = message_of(&run.event.request_json).into_owned();
        let fixed = fixed(&run.name);
        let pinned = Pinned::of(memory.window).map(|pinned| pinned.to_string());
        let repair = RepairContext::of(&run.policy).map(|context| context.to_string());
        let workspace = Workspace::of(&run.policy).map(|workspace| workspace.to_string());
        let tasks = Tasks::of(run, &message).map(|tasks| tasks.to_string());
        let instructions = [Some(fixed.clone()), pinned, repair, workspace, tasks]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(BLOCK_BREAK);
        Self {
            fixed,
            instructions,
            message,
        }
    }
}

/// The fleet's pinned memory: its own `core` entries, as many as the load's
/// byte budget holds, newest first.
struct Pinned<'w>(Vec<&'w MemoryDelta<'w>>);

impl<'w> Pinned<'w> {
    /// The pinned entries of `window`; `None` when it holds none, so the
    /// prompt carries no empty heading.
    fn of(window: &'w [MemoryDelta<'w>]) -> Option<Self> {
        let mut spent = 0_usize;
        let pinned: Vec<&MemoryDelta<'_>> = window
            .iter()
            .filter(|entry| entry.category == PINNED_CATEGORY)
            .take_while(|entry| {
                spent = spent.saturating_add(entry.bytes());
                spent <= LOAD_WINDOW_BYTES
            })
            .collect();
        (!pinned.is_empty()).then_some(Self(pinned))
    }
}

impl fmt::Display for Pinned<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{PINNED_MEMORY}")?;
        self.0
            .iter()
            .try_for_each(|entry| write!(f, "\n- {}: {}", entry.key, entry.content))
    }
}

/// What a write-bound run may publish.
///
/// Read from the daemon's binding and the ref rule it locked, never from the
/// event: the branch is the one value the model must not choose.
struct RepairContext<'p> {
    /// The one repository the binding names.
    repository: &'p str,
    /// The branch the locked `git/refs` rule admits.
    branch: &'p str,
    /// The branch a Pull Request opens into.
    base: &'p str,
}

impl<'p> RepairContext<'p> {
    /// The context `policy` authorises; `None` for a read-bound or unbound
    /// run, and for a write binding the daemon compiled no ref rule for.
    fn of(policy: &'p ExecutionPolicy<'_>) -> Option<Self> {
        let binding = policy
            .repository_binding
            .as_ref()
            .filter(|binding| binding.access == RepositoryAccess::Write)?;
        let [repository] = binding.repositories.as_slice() else {
            return None;
        };
        let refs = repository::path(repository, REFS_PATH);
        let branch = policy
            .http_origin_policies
            .iter()
            .flat_map(|origin| &origin.requests)
            .filter(|rule| rule.method == HttpMethod::Post && rule.path == refs)
            .flat_map(|rule| &rule.json_fields)
            .find(|field| field.name == FIELD_REF)?
            .string_value
            .as_deref()?
            .strip_prefix(REFS_HEADS)?;
        Some(Self {
            repository,
            branch,
            base: &binding.base_branch,
        })
    }
}

impl fmt::Display for RepairContext<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{TRUSTED_REPAIR_CONTEXT}\nrepository: {}\nrepair branch: {}\ntrusted base: {}",
            self.repository, self.branch, self.base
        )
    }
}

/// Where the supervisor checked the run's repositories out, so the model
/// starts in them instead of looking for them.
struct Workspace<'p>(Vec<Checkout<'p>>);

impl<'p> Workspace<'p> {
    /// The checkouts `policy` gets; `None` when it gets none, and for a
    /// binding that does not parse, which the supervisor refused before the
    /// turn began.
    fn of(policy: &'p ExecutionPolicy<'_>) -> Option<Self> {
        checkouts(policy)
            .ok()
            .filter(|checkouts| !checkouts.is_empty())
            .map(Self)
    }
}

impl fmt::Display for Workspace<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{WORKSPACE}")?;
        self.0.iter().try_for_each(|checkout| {
            let branch = if checkout.base.is_empty() {
                DEFAULT_BRANCH
            } else {
                checkout.base
            };
            write!(
                f,
                "\n{} is checked out at ./{} on {branch}, with origin set",
                checkout.repository, checkout.name
            )
        })
    }
}

/// What the task tools name: the person's message a span is taken from, and
/// the fleet's unfinished tasks a later message may steer.
struct Tasks<'p> {
    /// The run's head turn, the person's newest message.
    turn_id: &'p str,
    /// Its length in UTF-8 bytes, the end of a span that takes it whole.
    bytes: usize,
    /// The fleet's unfinished tasks, newest first.
    unfinished: &'p [DurableTaskInputCandidate],
}

impl<'p> Tasks<'p> {
    /// The block for `run`, whose message is `message`; `None` when the run
    /// is not offered the task tools.
    fn of(run: &'p RunPayload<'_>, message: &str) -> Option<Self> {
        crate::offer::names(run)
            .into_iter()
            .any(is_task_tool)
            .then(|| Self {
                turn_id: &run.event.turn_id,
                bytes: message.len(),
                unfinished: &run.unfinished_tasks,
            })
    }
}

impl fmt::Display for Tasks<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{DURABLE_TASKS}\nThe person's newest message is turn {}: task_start and task_input \
             name it as source_turn_id, with source_start and source_end as UTF-8 byte offsets \
             into it; 0 and {} take all of it.",
            self.turn_id, self.bytes
        )?;
        if self.unfinished.is_empty() {
            return write!(f, "\nThe fleet has no unfinished task.");
        }
        write!(f, "\nUnfinished tasks:")?;
        self.unfinished.iter().try_for_each(|task| {
            write!(
                f,
                "\n- {}: {} ({}, generation {})",
                task.durable_task_id,
                task.title,
                task.state.as_str(),
                task.generation
            )
        })
    }
}

#[cfg(test)]
mod tests;
