//! The daemon supplies recovery links and labels; model output never supplies them.

use afd_fleet_runtime::definition::POLICY_SEMANTICS;

use super::Observed;
use crate::types::{Action, ActionKind, Candidate, Check, CheckKind, CheckState};

impl Observed {
    pub(super) fn check(
        &mut self,
        kind: CheckKind,
        subject: &str,
        state: CheckState,
        action: Option<Action>,
    ) {
        self.checks.push(Check {
            kind,
            subject: subject.to_owned(),
            state,
            observed_at: self.now,
            next_action: if state == CheckState::Ready {
                None
            } else {
                action
            },
        });
    }

    pub(super) fn base(
        &mut self,
        draft: &super::Context,
        candidate: &Candidate,
        target: Option<&str>,
    ) {
        let definition = if candidate.definition.validate_for_save().is_ok() {
            CheckState::Ready
        } else {
            CheckState::Blocked
        };
        self.check(
            CheckKind::Definition,
            "definition",
            definition,
            Some(review_action(draft)),
        );
        let current_policy = candidate.policy_semantics == POLICY_SEMANTICS
            && (draft.target().is_none() || target.is_some());
        self.check(
            CheckKind::Policy,
            POLICY_SEMANTICS,
            if current_policy {
                CheckState::Ready
            } else {
                CheckState::Blocked
            },
            Some(review_action(draft)),
        );
    }
}

pub(crate) fn review_action(draft: &super::Context) -> Action {
    Action {
        kind: ActionKind::Review,
        label: "Review setup".to_owned(),
        href: draft.review_href(),
    }
}

pub(super) fn connections(draft: &super::Context) -> Action {
    Action {
        kind: ActionKind::WorkspaceConnections,
        label: "Open workspace connections".to_owned(),
        href: format!("/w/{}/connections", draft.workspace_id),
    }
}
