//! What a call that starts or steers a child asks of the loop, and what the
//! model reads back.
//!
//! A subagent tool parses its arguments into an [`Order`] and answers with
//! it; the loop runs the order inside that call, in the calling loop's place
//! ([`Caller`]), so a spawn is the caller's own child, narrowed from the
//! caller's tools and forked from the caller's turns. What the model reads is
//! small JSON, and the refusals are in the harness's own words.

use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};

use super::fork::forked;
use super::{Agents, Kind, Me, Status, TOOLS};
use crate::tool::{ErrorCode, ToolOutput};
use crate::transcript::Message;

/// What a child asking for a tool its parent lacks reads, after its name.
const NOT_HELD: &str = "is not one of this run's tools";
/// What a start past the run's caps reads.
const CAP_REACHED: &str = "as many children are open, or were started, as one run may; close one \
                           that is no longer needed";
/// The event a refused start logs under.
const EVENT_REFUSED: &str = "agent_refused";

/// A child the loop is asked to start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spawn {
    /// Its first task.
    pub message: String,
    /// The tools it holds, each one its parent holds; all of them when
    /// `None`.
    pub tools: Option<Vec<String>>,
    /// How much of its parent's history it starts from.
    pub fork: Fork,
}

/// How much of its parent's history a child starts from, as Codex's
/// `fork_turns` names it: `all`, `none`, or a number of turns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fork {
    /// Its task alone.
    None,
    /// Every turn of its parent's.
    All,
    /// Its parent's last this many turns.
    Last(NonZeroUsize),
}

impl Fork {
    /// `fork_turns` as the model wrote it, read the way Codex reads it:
    /// trimmed, `all` when blank, `none` and `all` in any case, or a
    /// number of turns from 1. `None` for anything else, which the call
    /// refuses.
    #[must_use]
    pub fn parse(fork_turns: &str) -> Option<Self> {
        let fork_turns = fork_turns.trim();
        if fork_turns.is_empty() || fork_turns.eq_ignore_ascii_case("all") {
            return Some(Self::All);
        }
        if fork_turns.eq_ignore_ascii_case("none") {
            return Some(Self::None);
        }
        fork_turns.parse().ok().map(Self::Last)
    }
}

/// What one subagent call asks of the loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Start a child.
    Spawn(Spawn),
    /// Queue a message for a child.
    Send {
        /// The child.
        target: u64,
        /// What it reads.
        message: String,
    },
    /// Give a child another task.
    FollowUp {
        /// The child.
        target: u64,
        /// Its task.
        message: String,
    },
    /// Wait for a child's turn to end.
    Wait {
        /// The child.
        target: u64,
        /// How long to wait at most.
        timeout: Duration,
    },
    /// List the run's children.
    List,
    /// Close a child.
    Close {
        /// The child.
        target: u64,
    },
    /// Interrupt a child's turn.
    Interrupt {
        /// The child.
        target: u64,
    },
}

/// An [`Action`] on one run's children.
///
/// A tool that starts or steers a child answers with one
/// ([`ToolOutput::ordering`]). The loop runs it in the calling loop's place,
/// inside the call, and the model reads what it answers. The `tools`
/// feature's seven tools are built on it; an embedder without that feature
/// writes its own the same way.
#[derive(Debug, Clone)]
pub struct Order {
    agents: Agents,
    action: Action,
}

impl PartialEq for Order {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.agents.0, &other.agents.0) && self.action == other.action
    }
}

impl Eq for Order {}

/// The loop an order runs in place of: who it is, the tools it holds, and,
/// for a spawn that forks, its transcript before the step that asked.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Caller<'a> {
    pub(crate) me: Me,
    pub(crate) tools: &'a [String],
    pub(crate) history: &'a [Message],
}

/// Why a child was not started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// Its parent holds no tool by this name, so it may not either.
    NotHeld(String),
    /// As many children are open, or were started, as the run's caps allow,
    /// or the run's children are paused.
    Capped,
}

impl Refused {
    /// The code the model reads with the refusal.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::NotHeld(_) => ErrorCode::CHILD_TOOL_NOT_HELD,
            Self::Capped => ErrorCode::CHILD_CAP_REACHED,
        }
    }
}

/// A refused child, as the call that asked for it reads it.
impl From<Refused> for ToolOutput {
    fn from(refused: Refused) -> Self {
        let code = refused.code();
        match refused {
            Refused::NotHeld(name) => Self::failed(code, &format!("{name} {NOT_HELD}")),
            Refused::Capped => Self::failed(code, CAP_REACHED),
        }
    }
}

impl Order {
    /// `action`, on the children `agents` keeps.
    #[must_use]
    pub const fn new(agents: Agents, action: Action) -> Self {
        Self { agents, action }
    }

    /// Whether running it reads the caller's tools and transcript.
    pub(crate) const fn spawns(&self) -> bool {
        matches!(self.action, Action::Spawn(_))
    }

    /// Runs the order in `caller`'s place, and answers what the model reads.
    pub(crate) async fn run(self, caller: Caller<'_>) -> ToolOutput {
        let agents = &self.agents;
        match self.action {
            Action::Spawn(spawn) => match start(agents, caller, spawn) {
                Ok(id) => answer(&json!({ "agent_id": id })),
                Err(refused) => {
                    let error_code = refused.code().as_str();
                    let event = EVENT_REFUSED;
                    tracing::debug!(error_code, event);
                    refused.into()
                }
            },
            Action::Send { target, message } => agents
                .send(target, message)
                .map_or_else(|| not_found(target), accepted),
            Action::FollowUp { target, message } => agents
                .follow_up(target, message)
                .map_or_else(|| not_found(target), accepted),
            Action::Wait { target, timeout } => agents
                .wait(target, Some(timeout))
                .await
                .map_or_else(|| not_found(target), |status| answer(&state(&status))),
            Action::List => answer(&Value::Array(
                agents.list().iter().map(listed).collect::<Vec<_>>(),
            )),
            Action::Close { target } => agents
                .close(target)
                .map_or_else(|| not_found(target), previous),
            Action::Interrupt { target } => agents
                .interrupt(target)
                .map_or_else(|| not_found(target), previous),
        }
    }
}

/// Starts `spawn` as `caller`'s child: the tools it names, each one the
/// caller holds, or all of them; none of the seven at the depth cap; and the
/// caller's last turns when it forks.
fn start(agents: &Agents, caller: Caller<'_>, spawn: Spawn) -> Result<u64, Refused> {
    let mut tools = match spawn.tools {
        None => caller.tools.to_vec(),
        Some(named) => {
            if let Some(missing) = named.iter().find(|name| !caller.tools.contains(name)) {
                return Err(Refused::NotHeld(missing.clone()));
            }
            named
        }
    };
    if caller.me.depth.saturating_add(1) >= agents.caps().depth {
        tools.retain(|name| !TOOLS.contains(&name.as_str()));
    }
    let opening = forked(caller.history, spawn.fork);
    agents.reserve(caller.me, tools, opening, spawn.message)
}

/// `value` as a succeeded call's text: JSON, which these shapes always are.
fn answer(value: &Value) -> ToolOutput {
    ToolOutput::succeeded(value.to_string())
}

/// Whether a message for a child was taken.
fn accepted(accepted: bool) -> ToolOutput {
    answer(&json!({ "accepted": accepted }))
}

/// The status a child had before a close or an interrupt.
fn previous(kind: Kind) -> ToolOutput {
    answer(&json!({ "previous_status": kind.as_str() }))
}

/// Where a child is, as a wait answers: its answer once done, its detail
/// once failed.
fn state(status: &Status) -> Value {
    let kind = status.kind().as_str();
    match status {
        Status::Done(text) => json!({ "status": kind, "answer": text }),
        Status::Failed(detail) => json!({ "status": kind, "detail": detail }),
        Status::Running | Status::Interrupted | Status::Closed => json!({ "status": kind }),
    }
}

/// One child, as `list_agents` answers it.
fn listed(child: &super::Listed) -> Value {
    json!({
        "agent_id": child.agent_id,
        "status": child.status.as_str(),
        "depth": child.depth,
        "calls": child.calls,
    })
}

/// What a call naming a child the run does not have reads.
fn not_found(id: u64) -> ToolOutput {
    ToolOutput::failed(
        ErrorCode::CHILD_NOT_FOUND,
        &format!("no child of this run has id {id}"),
    )
}
