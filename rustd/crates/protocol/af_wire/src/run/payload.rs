//! Installed fleet work and the history it carries.

use crate::{policy::ExecutionPolicy, tool_detail::ItemStatus, turn::TurnEnvelope};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// A finished run that left no reply.
pub const ANSWER_NONE: &str = "[no reply]";
/// What opens a failed run's answer; the run's failure label follows it.
pub const ANSWER_FAILED: &str = "[the run failed: ";
/// What closes a failed run's answer.
pub const ANSWER_FAILED_END: &str = "]";

/// One earlier exchange in the conversation: what was asked, the tool calls
/// its run made, and what the fleet answered.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Turn<'a> {
    /// The turn's place in its thread (`thread.turns.sequence`). A saved
    /// summary's watermark counts in it, and a run skips the turns a loaded
    /// summary covers.
    pub sequence: u64,
    /// The turn's message, as `turn::message_of` reads it.
    #[serde(borrow)]
    pub message: Cow<'a, str>,
    /// Every tool call the turn's run made, in the order it made them, as
    /// `thread.items` keeps them; empty for a turn that called none.
    #[serde(borrow, default, skip_serializing_if = "Vec::is_empty")]
    pub calls: Vec<EarlierCall<'a>>,
    /// The fleet's answer, or the fixed text a reply-less or failed run reads as.
    #[serde(borrow)]
    pub answer: Cow<'a, str>,
}

/// One tool call an earlier turn's run made: what it called, with what, and
/// what came back, as the model read it then.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EarlierCall<'a> {
    /// The call's `{fence}:{n}` identifier, as the thread names it.
    #[serde(borrow)]
    pub call_id: Cow<'a, str>,
    /// The tool it called.
    #[serde(borrow)]
    pub tool: Cow<'a, str>,
    /// Every argument the call was made with, secret values masked.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub arguments: serde_json::Map<String, serde_json::Value>,
    /// What the call returned, secret values masked.
    #[serde(borrow)]
    pub output: Cow<'a, str>,
    /// How it ended.
    pub status: ItemStatus,
}

/// Work assigned to a runner. Return its `fencing_token` with updates; the server rejects
/// older tokens after reassignment.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunPayload<'a> {
    /// Identifier for this run.
    #[serde(borrow)]
    pub run_id: Cow<'a, str>,
    /// Monotonic guard a report must echo.
    pub fencing_token: u64,
    /// Immutable setup proved and retained by the issue transaction.
    #[serde(borrow, default, skip_serializing_if = "Option::is_none")]
    pub setup_revision_id: Option<Cow<'a, str>>,
    /// Exact committed ordinary-turn state; private bytes require a fenced native read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_resume: Option<super::TurnResume>,
    /// The instant after which the turn becomes reassignable.
    #[serde(with = "crate::instant")]
    pub run_expires_at: jiff::Timestamp,
    /// The turn to run.
    #[serde(borrow)]
    pub event: TurnEnvelope<'a>,
    /// What the run is permitted to do.
    #[serde(borrow)]
    pub policy: ExecutionPolicy<'a>,
    /// The installed fleet's behaviour prose, so the sandboxed turn runs the
    /// installed behaviour rather than a generic one. Soft reasoning input —
    /// hard tool and secret policy stays in `policy`.
    #[serde(borrow)]
    pub instructions: Cow<'a, str>,
    /// Every finished earlier turn of the conversation, oldest first, ahead
    /// of this turn; empty unless the turn is a chat message. Only the run
    /// frame bounds it: an assignment that would not fit one drops its
    /// oldest turns.
    #[serde(borrow)]
    pub history: Vec<Turn<'a>>,
    /// True when the runner can reuse its latest held sandbox for a new turn. Reassigned
    /// turns and expired or outdated sandboxes require a fresh sandbox.
    pub resume_hold: bool,
    /// Reviewed runner decision selection frozen at issue; no provider key.
    /// Absent on hosts without an enabled reviewed selection.
    pub decision: Option<Box<crate::decision::DecisionAssignment>>,
    /// Native task ownership and saved recovery cursors, frozen at issuance.
    #[serde(borrow, default, skip_serializing_if = "Option::is_none")]
    pub task: Option<crate::task::TaskContext<'a>>,
    /// Owner-authored task policy requires intent before any investigation or checkout.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub durable_tasks: bool,
    /// The fleet's memory window, as `GET /v1/runners/me/memory/{fleet_id}`
    /// returns it. A daemon that reads a report's `memory` sends it on every
    /// run, an empty window included. It is absent when the read failed; the
    /// runner then reads memory itself.
    #[serde(borrow, default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<crate::memory::MemoryLoadResponse<'a>>,
    /// Whether a later message waited in the run's conversation when the
    /// daemon assigned it. False lets the run skip its first fold ask; a
    /// message that lands after the assignment folds on the ask after the answer.
    pub inputs_waiting: bool,
    /// Whose work this is. A fleet's run names its fleet, the one
    /// `event.fleet_id` names. A draft's run names the draft and carries its
    /// setup turn, and its `event` is the draft's message: `turn_id` the
    /// message, `fleet_id` the draft.
    #[serde(borrow)]
    pub owner: super::Owner<'a>,
}
