//! Installed fleet work and the bounded history it carries.

use crate::{event::EventEnvelope, policy::ExecutionPolicy};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// The most earlier turns a chat run carries.
pub const HISTORY_TURNS_MAX: usize = 8;
/// The most bytes a chat run's turns carry, messages and answers together.
pub const HISTORY_BYTES_MAX: usize = 65_536;
/// The most bytes one turn's message, or its answer, carries.
pub const TURN_TEXT_BYTES_MAX: usize = 16_384;
/// A finished run that left no reply.
pub const ANSWER_NONE: &str = "[no reply]";
/// What opens a failed run's answer; the run's failure label follows it.
pub const ANSWER_FAILED: &str = "[the run failed: ";
/// What closes a failed run's answer.
pub const ANSWER_FAILED_END: &str = "]";

/// One earlier exchange in the fleet's thread: what was asked, and what the
/// fleet answered.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Turn<'a> {
    /// The event's message, as `event::message_of` reads it.
    #[serde(borrow)]
    pub message: Cow<'a, str>,
    /// The fleet's answer, or the fixed text a reply-less or failed run reads as.
    #[serde(borrow)]
    pub answer: Cow<'a, str>,
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
    /// The instant after which the event becomes reclaimable.
    #[serde(with = "crate::instant")]
    pub run_expires_at: jiff::Timestamp,
    /// The event to run.
    #[serde(borrow)]
    pub event: EventEnvelope<'a>,
    /// What the run is permitted to do.
    #[serde(borrow)]
    pub policy: ExecutionPolicy<'a>,
    /// The installed fleet's behaviour prose, so the sandboxed turn runs the
    /// installed behaviour rather than a generic one. Soft reasoning input —
    /// hard tool and secret policy stays in `policy`.
    #[serde(borrow)]
    pub instructions: Cow<'a, str>,
    /// The fleet's earlier turns, oldest first, ahead of this event; empty
    /// unless the event is a chat message.
    #[serde(borrow)]
    pub history: Vec<Turn<'a>>,
    /// True when the runner can reuse its latest held sandbox for a new event. Reassigned
    /// events and expired or outdated sandboxes require a fresh sandbox.
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
    pub memory: Option<crate::memory::MemoryHydrateResponse<'a>>,
    /// Whether a later message waited in the run's conversation when the
    /// daemon claimed it. False lets the run skip its first fold ask; a
    /// message that lands after the claim folds on the ask after the answer.
    pub inputs_waiting: bool,
    /// Whose work this is. Absent on a fleet's run, whose owner is the fleet
    /// `event.fleet_id` names. A draft's run names the draft here, and its
    /// `event` is the draft's message: `event_id` the message, `fleet_id` the
    /// draft. Offered only to a protocol v3 runner.
    #[serde(borrow, default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<super::Owner<'a>>,
    /// A draft's setup turn: present exactly when `owner` names a draft.
    #[serde(borrow, default, skip_serializing_if = "Option::is_none")]
    pub setup: Option<super::SetupWork<'a>>,
}
