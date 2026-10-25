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

use garde::Validate;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

/// The spellings `fleet.approval_gates.status` stores.
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

/// Every state a gate row's `status` can be in.
///
/// One enum for both planes, beside the spellings it renders: the runner plane
/// opens a row at [`GateStatus::Pending`] and reads the durable answer back,
/// and the operator plane filters its inbox by it. Two copies would let a row
/// one plane wrote be a state the other cannot name. A reader parses a stored
/// spelling through serde (`afd_core::spelling::from_spelling`), so the
/// `rename_all` below is the only reading vocabulary and [`GateStatus::as_str`]
/// the only writing one, pinned against each other by this module's tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateStatus {
    /// Still waiting on a human.
    Pending,
    /// A reviewer approved it.
    Approved,
    /// A reviewer refused it.
    Denied,
    /// The deadline passed with no answer.
    TimedOut,
    /// The daemon stopped the fleet.
    AutoKilled,
}

impl GateStatus {
    /// The spelling the `status` column stores.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => status::PENDING,
            Self::Approved => status::APPROVED,
            Self::Denied => status::DENIED,
            Self::TimedOut => status::TIMED_OUT,
            Self::AutoKilled => status::AUTO_KILLED,
        }
    }
}

/// One gate as the inbox lists it.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
    pub gate_type: Cow<'a, str>,
    /// What the fleet proposes, in a person's words.
    pub proposed_action: Cow<'a, str>,
    /// How far the consequences reach.
    pub blast_radius: Cow<'a, str>,
    /// Where the gate stands.
    pub status: Cow<'a, str>,
    /// The resolver's note, empty while pending.
    pub detail: Cow<'a, str>,
    /// When it was raised.
    #[serde(with = "afd_wire::instant")]
    pub created_at: jiff::Timestamp,
    /// When it stops waiting.
    #[serde(with = "afd_wire::instant")]
    pub expires_at: jiff::Timestamp,
    /// When it was resolved, or `null` while pending.
    #[serde(with = "afd_wire::instant::option", default)]
    pub resolved_at: Option<jiff::Timestamp>,
    /// Who resolved it, empty while pending.
    pub resolved_by: Cow<'a, str>,
    /// Resolver's display name, or an empty string when unavailable. Fall back to
    /// `resolved_by` when empty.
    pub resolved_by_name: Cow<'a, str>,
    /// The evidence behind the proposal, verbatim.
    //
    // `null` for a row whose stored text will not parse. That cannot happen
    // through the park, which writes valid JSON, so a null here is corruption
    // — and showing the gate WITHOUT its evidence beats failing the whole
    // queue, because an operator can still read what is being asked and deny
    // it, where a failed read hides every other gate too.
    //
    // Serialized it is whatever document the gate was opened with; in Rust it
    // is an unparsed slice, which has no schema of its own. `value_type` names
    // that difference.
    #[cfg_attr(feature = "openapi", schema(value_type = Option<Object>))]
    pub evidence: Option<&'a RawValue>,
}

/// `GET /v1/workspaces/{workspace_id}/approvals` — the queue.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize)]
pub struct ApprovalsResponse<'a> {
    /// The gates on this page, newest first.
    pub items: Vec<ApprovalSummary<'a>>,
    /// Where the next page resumes, or `null` on the last one.
    pub next_cursor: Option<Cow<'a, str>>,
}

/// The body a decision may carry.
///
/// Every field is optional, and so is the whole body. A decision is complete
/// without a note, and demanding one would make the common answer the awkward
/// one.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Default, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ResolveApprovalRequest<'a> {
    /// The operator's note, stored as the gate's `detail`.
    #[serde(borrow, default)]
    // `inner` reaches through the `Option`: an absent note and an empty one are
    // the same answer, so only a note that was SENT is bounded.
    #[garde(inner(length(bytes, max = APPROVAL_REASON_MAX_BYTES)))]
    pub reason: Option<Cow<'a, str>>,
}

/// The longest note an operator may file with a gate answer.
///
/// `REASON_MAX`, mirrored: the column is unbounded text and a note is a
/// sentence, so the cap is what keeps a decision from becoming storage.
pub const APPROVAL_REASON_MAX_BYTES: usize = 4096;

/// What answering a gate reports, whether or not this caller won the race.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize)]
pub struct ResolvedResponse<'a> {
    /// The gate that was answered.
    pub gate_id: Cow<'a, str>,
    /// The action it gated.
    pub action_id: Cow<'a, str>,
    /// Where it now stands.
    pub outcome: Cow<'a, str>,
    /// When it was decided.
    #[serde(with = "afd_wire::instant")]
    pub resolved_at: jiff::Timestamp,
    /// Who decided it — not necessarily this caller.
    pub resolved_by: Cow<'a, str>,
}

#[cfg(test)]
mod tests {
    use super::GateStatus;

    /// Every state, in the order the column's lifecycle reaches them.
    const EVERY: [GateStatus; 5] = [
        GateStatus::Pending,
        GateStatus::Approved,
        GateStatus::Denied,
        GateStatus::TimedOut,
        GateStatus::AutoKilled,
    ];

    /// The spelling written is the spelling read, for every state: a drift
    /// would write rows the next read could not name, and a gate would sit
    /// pending forever with a human's answer landing nowhere.
    #[test]
    fn every_state_reads_back_from_the_spelling_it_writes() {
        for state in EVERY {
            let written = serde_json::Value::from(state.as_str());
            assert_eq!(serde_json::to_value(state).ok(), Some(written.clone()));
            assert_eq!(
                serde_json::from_value::<GateStatus>(written).ok(),
                Some(state)
            );
        }
    }

    #[test]
    fn the_column_spellings_are_pinned() {
        let spelled: Vec<_> = EVERY.map(GateStatus::as_str).into();
        // pin test: literal is the contract
        assert_eq!(
            spelled,
            ["pending", "approved", "denied", "timed_out", "auto_killed"]
        );
    }

    #[test]
    fn a_spelling_no_row_carries_is_not_a_state() {
        let unknown = serde_json::from_value::<GateStatus>("elsewhere".into());
        assert_eq!(unknown.ok(), None);
    }
}
