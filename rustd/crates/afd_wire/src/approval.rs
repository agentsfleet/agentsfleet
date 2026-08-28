//! The approval inbox's payloads: the queue, one gate, and the answer.
//!
//! # Evidence rides as raw JSON
//!
//! `evidence` is a `&RawValue`, not a parsed `Value`. The column holds whatever
//! the tool that raised the gate wrote, and a person is being asked to judge
//! exactly those bytes — re-serializing them would normalise a payload the
//! operator is supposed to read verbatim before deciding.
//!
//! # An already-answered gate is not an error shape
//!
//! `ResolvedResponse` is what a winning decision AND a losing one both carry,
//! because the second caller wanted the gate resolved and it is. What differs
//! is the attribution: `resolved_by` names whoever actually decided, which is
//! why the field is on the response rather than assumed by the caller.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

/// The spellings `core.fleet_approval_gates.status` stores.
///
/// Declared HERE, in the crate both planes already read, because two crates
/// write and read this one column: the runner plane parks a gate at `PENDING`
/// and reads the durable answer back, and the operator plane writes the answer.
/// A drift between their two copies would make a row one of them wrote the
/// other could not read — and the gate would sit pending forever with a human's
/// answer landing nowhere.
pub mod status {
    /// Still waiting on a human.
    pub const PENDING: &str = "pending";
    /// A reviewer approved it.
    pub const APPROVED: &str = "approved";
    /// A reviewer refused it.
    pub const DENIED: &str = "denied";
    /// The deadline passed with no answer.
    pub const TIMED_OUT: &str = "timed_out";
    /// The daemon stopped the fleet.
    pub const AUTO_KILLED: &str = "auto_killed";
}

/// One gate as the inbox lists it.
#[derive(Debug, Clone, Serialize)]
pub struct ApprovalSummary<'a> {
    /// The gate's own row id — what a decision addresses it by.
    pub gate_id: Cow<'a, str>,
    /// The fleet that raised it.
    pub fleet_id: Cow<'a, str>,
    /// That fleet's name, joined at read time so a rename is visible.
    pub fleet_name: Cow<'a, str>,
    /// The workspace it belongs to.
    pub workspace_id: Cow<'a, str>,
    /// The action it gates.
    pub action_id: Cow<'a, str>,
    /// The tool asking.
    pub tool_name: Cow<'a, str>,
    /// The verb it wants.
    pub action_name: Cow<'a, str>,
    /// Which family of gate this is.
    pub gate_kind: Cow<'a, str>,
    /// What the fleet proposes, in a person's words.
    pub proposed_action: Cow<'a, str>,
    /// How far the consequences reach.
    pub blast_radius: Cow<'a, str>,
    /// Where the gate stands.
    pub status: Cow<'a, str>,
    /// The resolver's note, empty while pending.
    pub detail: Cow<'a, str>,
    /// When it was raised.
    pub created_at: i64,
    /// When it stops waiting.
    pub timeout_at: i64,
    /// When it was resolved, or `null` while pending.
    pub updated_at: Option<i64>,
    /// Who resolved it, empty while pending.
    pub resolved_by: Cow<'a, str>,
    /// The evidence behind the proposal, verbatim.
    ///
    /// `null` for a row whose stored text will not parse. That cannot happen
    /// through the park, which writes valid JSON, so a null here is corruption
    /// — and showing the gate WITHOUT its evidence beats failing the whole
    /// queue, because an operator can still read what is being asked and deny
    /// it, where a failed read hides every other gate too.
    pub evidence: Option<&'a RawValue>,
}

/// `GET /v1/workspaces/{workspace_id}/approvals` — the queue.
#[derive(Debug, Clone, Serialize)]
pub struct ApprovalsResponse<'a> {
    /// The gates on this page, oldest first.
    pub items: Vec<ApprovalSummary<'a>>,
    /// Where the next page resumes, or `null` on the last one.
    pub next_cursor: Option<Cow<'a, str>>,
}

/// The body a decision may carry.
///
/// Every field optional, and the whole body optional with it: a decision is
/// complete without a note, and demanding one would make the common answer the
/// awkward one.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ResolveApprovalRequest<'a> {
    /// The operator's note, stored as the gate's `detail`.
    #[serde(borrow, default)]
    pub reason: Option<Cow<'a, str>>,
}

/// What answering a gate reports, whether or not this caller won the race.
#[derive(Debug, Clone, Serialize)]
pub struct ResolvedResponse<'a> {
    /// The gate that was answered.
    pub gate_id: Cow<'a, str>,
    /// The action it gated.
    pub action_id: Cow<'a, str>,
    /// Where it now stands.
    pub outcome: Cow<'a, str>,
    /// When it was decided.
    pub resolved_at: i64,
    /// Who decided it — not necessarily this caller.
    pub resolved_by: Cow<'a, str>,
}
