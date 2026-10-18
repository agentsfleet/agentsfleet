//! Private projections contain only creator-visible setup and conversation state.

use afd_core::id::Uuid7;
use afd_fleet_runtime::Definition;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};

pub(crate) const EDITING: &str = "editing";
pub(crate) const SAVING: &str = "saving";
pub(crate) const CLOSED: &str = "closed";
pub(crate) const QUEUED: &str = "queued";
pub(crate) const RUNNING: &str = "running";
pub(crate) const CANCELLED: &str = "cancelled";

/// Draft lifetime, independent of individual message execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DraftState {
    /// Chat and idle candidate editing are available.
    Editing,
    /// A fleet setup operation has reserved this candidate.
    Saving,
    /// Activation closed the conversation; retention is bounded.
    Closed,
}

impl DraftState {
    /// The lifecycle vocabulary used in rows and conflict responses.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Editing => EDITING,
            Self::Saving => SAVING,
            Self::Closed => CLOSED,
        }
    }
}

/// Durable state of a user request and its optional assistant answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageState {
    /// Accepted and awaiting bounded admission.
    Queued,
    /// An unexpired authoring attempt owns the request.
    Running,
    /// Terminal answer and known accounting were recorded.
    Completed,
    /// A cutoff or deletion fenced further private output.
    Cancelled,
    /// A durable bounded failure explains the recovery action.
    Failed,
    /// The attempt reached its deadline.
    TimedOut,
}

impl MessageState {
    /// The durable state vocabulary also used by both chat clients.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => QUEUED,
            Self::Running => RUNNING,
            Self::Completed => "completed",
            Self::Cancelled => CANCELLED,
            Self::Failed => "failed",
            Self::TimedOut => "timed_out",
        }
    }
}

/// Lightweight creator-only collection item with no candidate or transcript.
#[derive(Debug, Serialize)]
pub struct DraftCard {
    /// Private draft identity.
    pub id: Uuid7,
    /// Display title.
    pub title: String,
    /// Current lifecycle.
    pub state: DraftState,
    /// Strong version for conditional edits.
    pub etag: String,
    /// Related fleet, when available.
    pub linked_fleet_id: Option<Uuid7>,
    /// Last mutation time.
    #[serde(with = "afd_wire::instant")]
    pub updated_at: Timestamp,
    /// Private retention deadline.
    #[serde(with = "afd_wire::instant::option")]
    pub expires_at: Option<Timestamp>,
    /// The current candidate's revision, before the chat proposes one.
    pub candidate_revision: Option<i64>,
    /// The saved fleet's current setup revision, while a fleet is saved.
    pub setup_revision: Option<i64>,
    /// The Library revision this chat published, once it has.
    pub publication_revision: Option<i64>,
}

/// Immutable normalized setup accepted from the drawer or proposal tool.
#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    /// Fresh daemon-derived identity for this exact version.
    pub id: Uuid7,
    /// Draft-local decimal revision, starting at one.
    pub revision: String,
    /// Full setup; contains no permission receipts or credentials.
    pub definition: Definition,
    /// Daemon-computed content identity.
    pub canonical_digest: String,
    /// Meaning of the policy schema, pinned independently of content.
    pub policy_semantics: String,
}

/// A creator's mutable draft with its current immutable candidate.
#[derive(Debug, Clone, Serialize)]
pub struct Draft {
    /// Private conversation identity.
    pub id: Uuid7,
    /// Draft lifetime.
    pub state: DraftState,
    /// Human title, independent of authored slugs.
    pub title: String,
    /// Strong opaque resource version.
    pub etag: String,
    /// Explicit selection for future messages.
    pub authoring_model_ref: String,
    /// Monotone stopped-through message watermark.
    pub cancelled_through: String,
    /// Selected candidate, absent before initialization.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate: Option<Candidate>,
    /// Effective setup and current readiness, computed by the setup reader.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review: Option<super::Review>,
    /// Shared fleet whose setup this draft improves.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linked_fleet_id: Option<Uuid7>,
    /// Receipt of an explicit publication of this creator's candidate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publication: Option<super::Publication>,
    /// Time activation closed the draft.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(with = "afd_wire::instant::option")]
    pub closed_at: Option<Timestamp>,
    /// Private retention deadline.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(with = "afd_wire::instant::option")]
    pub expires_at: Option<Timestamp>,
}

/// Durable message acceptance, independent of execution or completion.
#[derive(Debug, Clone, Serialize)]
pub struct Accepted {
    /// Always the accepted vocabulary, never a completion claim.
    pub status: &'static str,
    /// Request sequence within this draft.
    pub sequence: String,
    /// Whether this operation had already committed.
    pub replayed: bool,
}

/// Creation keeps the complete draft projection at the top level.
#[derive(Debug, Clone, Serialize)]
pub struct Created {
    /// The new or recovered private draft.
    #[serde(flatten)]
    pub draft: Draft,
    /// Present only when creation accepted a first message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accepted: Option<Accepted>,
}

/// One role in a message pair; assistant output shares the request sequence.
#[derive(Debug, Clone, Serialize)]
pub struct Message {
    /// Decimal sequence naming the user request.
    pub sequence: String,
    /// Body operation key originally accepted.
    pub operation_id: Uuid7,
    /// Either user or assistant; derived by the service.
    pub role: &'static str,
    /// Durable execution verdict.
    pub state: MessageState,
    /// Private user text or retained assistant answer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Candidate selected by the accepted assistant result.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate_id: Option<Uuid7>,
    /// Daemon-authoritative provider knowledge and cumulative runtime charge.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<afd_wire::authoring::Usage>,
    /// Bounded persistent recovery action on `failed`/`timed_out` messages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<super::Failure>,
    /// Request acceptance time.
    #[serde(with = "afd_wire::instant")]
    pub created_at: Timestamp,
    /// Terminal time, absent while queued/running.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(with = "afd_wire::instant::option")]
    pub finished_at: Option<Timestamp>,
}

/// A bounded keyset page; absence of a total is explicitly represented as null.
#[derive(Debug, Serialize)]
pub struct Page<T> {
    /// Current bounded page.
    pub items: Vec<T>,
    /// No table-wide count is performed for private conversation reads.
    pub total: Option<u64>,
    /// Opaque boundary, null on the final page.
    pub next_cursor: Option<String>,
}
