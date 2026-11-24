//! Task reads expose confirmed work and its completion statements independently of chat.

use std::borrow::Cow;

use af_wire::durable_task::{DurableTaskContext, DurableTaskState};
use serde::{Deserialize, Serialize};

/// A bounded page of unfinished work belonging to one fleet.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DurableTasksResponse<'a> {
    /// Newest work first; completed work remains available by its identity.
    #[serde(borrow)]
    pub items: Vec<DurableTaskSummary<'a>>,
    /// Opaque keyset boundary, absent when no later work exists.
    #[serde(borrow)]
    pub next_cursor: Option<Cow<'a, str>>,
}

/// Compact native state for the fleet's current-work panel.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DurableTaskSummary<'a> {
    /// Stable objective identifier.
    #[serde(borrow)]
    pub id: Cow<'a, str>,
    /// User-facing work title.
    #[serde(borrow)]
    pub title: Cow<'a, str>,
    /// Confirmed durable lifecycle state.
    pub status: DurableTaskState,
    /// Monotonic native resource version.
    pub resource_version: u64,
    /// Current task ownership generation.
    pub generation: u64,
    /// Native creation instant.
    #[serde(with = "af_wire::instant")]
    pub created_at: jiff::Timestamp,
    /// Existing cumulative deadline.
    #[serde(with = "af_wire::instant")]
    pub expires_at: jiff::Timestamp,
}

/// Task detail with its completion statements and cumulative progress.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DurableTaskResponse<'a> {
    /// Current native dependency and saved check results; no remote refresh is implied.
    pub dependency: Option<af_wire::durable_task::DurableTaskDependency>,
    /// Latest publication receipt; only a confirmed receipt establishes a published head.
    pub publication: Option<af_wire::durable_task::DurableTaskEffect>,
    /// Strong version shared with the response header and conditional writes.
    #[serde(borrow)]
    pub etag: Cow<'a, str>,
    /// User-facing title.
    #[serde(borrow)]
    pub title: Cow<'a, str>,
    /// Version used by the strong resource tag.
    pub resource_version: u64,
    /// Accepted objective, completion statements, cursors and remaining limits.
    #[serde(borrow)]
    pub context: DurableTaskContext<'a>,
}

/// Explicit human lifecycle control; completion requires native goal proof.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DurableTaskControlState {
    /// Stop new work while retaining progress.
    Paused,
    /// Resume paused or blocked work within its remaining allowance.
    Ready,
    /// Stop new work permanently, retaining uncertain effects for reconciliation.
    Cancelled,
}

/// A person's conditional lifecycle control, or their edit of the task's
/// completion statements.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, garde::Validate)]
#[serde(untagged, deny_unknown_fields)]
pub enum PatchDurableTaskRequest {
    /// Pause, resume or cancel without changing the accepted objective.
    Control {
        /// Requested native lifecycle state.
        #[garde(skip)]
        state: DurableTaskControlState,
        /// Generation from the task's current native context.
        #[garde(range(min = 1))]
        expected_generation: u64,
    },
    /// Replace the statements the task is judged done against.
    Statements {
        /// At least one statement, each with a unique id.
        #[garde(
            length(min = 1),
            dive,
            custom(af_wire::durable_task::unique_statement_ids)
        )]
        questions: Vec<af_wire::durable_task::DurableTaskStatement>,
        /// Generation read before the edit; a stale one is refused.
        #[garde(range(min = 1))]
        expected_generation: u64,
    },
}

#[cfg(test)]
mod tests {
    use garde::Validate as _;

    use super::{DurableTaskControlState, PatchDurableTaskRequest};

    fn patch(body: &str) -> Option<PatchDurableTaskRequest> {
        serde_json::from_str(body).ok()
    }

    const STATEMENT: &str = r#"{"id": "pr_green", "question": "Are the checks green?", "criteria": {"true": "All passed.", "false": "One failed."}}"#;

    #[test]
    fn a_person_edits_the_statements_under_a_generation() {
        let edit = patch(&format!(
            r#"{{"questions": [{STATEMENT}], "expected_generation": 4}}"#
        ));
        assert!(matches!(
            &edit,
            Some(PatchDurableTaskRequest::Statements { questions, expected_generation: 4 })
                if questions.len() == 1
        ));
        assert!(edit.is_some_and(|edit| edit.validate().is_ok()));
    }

    #[test]
    fn a_control_still_reads_its_state() {
        assert!(matches!(
            patch(r#"{"state": "paused", "expected_generation": 2}"#),
            Some(PatchDurableTaskRequest::Control {
                state: DurableTaskControlState::Paused,
                expected_generation: 2
            })
        ));
    }

    #[test]
    fn an_empty_duplicate_or_stale_shaped_edit_is_refused() {
        for body in [
            r#"{"questions": [], "expected_generation": 4}"#.to_owned(),
            format!(r#"{{"questions": [{STATEMENT}, {STATEMENT}], "expected_generation": 4}}"#),
            format!(r#"{{"questions": [{STATEMENT}], "expected_generation": 0}}"#),
        ] {
            assert!(
                patch(&body).is_some_and(|edit| edit.validate().is_err()),
                "{body}"
            );
        }
    }

    #[test]
    fn goal_adoption_from_a_revision_no_longer_decodes() {
        let adopt = r#"{"goal_revision_id": "019a0000-0000-7000-8000-000000000001",
            "expected_generation": 3, "operation_id": "019a0000-0000-7000-8000-000000000002"}"#;
        assert!(patch(adopt).is_none());
        assert!(patch(&format!(r#"{{"questions": [{STATEMENT}]}}"#)).is_none());
    }
}
