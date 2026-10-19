//! Installation checks report current prerequisites, never inferred grant approval.

use super::{ACTIVE, KILLED, stored::Stored};
use crate::types::{Action, Check, CheckKind, CheckState, Review};
use serde::Serialize;

/// A check whose prerequisite currently holds.
const CHECK_READY: &str = "ready";
/// A check still waiting on a prerequisite somebody can supply.
const CHECK_PENDING: &str = "pending";
/// A check that cannot pass without a new review or another decision.
const CHECK_BLOCKED: &str = "blocked";
/// The setup boundary a changed review is reported under.
const KIND_POLICY: &str = "policy";
/// Setup a Kill cancelled; its draft returned to editing.
const SETUP_CANCELLED: &str = "cancelled";
/// Setup finished: the fleet is active.
const SETUP_READY: &str = "ready";
/// Setup holding on at least one check that is not ready.
const SETUP_WAITING: &str = "waiting";
/// Setup whose checks all hold, before activation commits.
const SETUP_CHECKING: &str = "checking";

/// One bounded setup observation.
#[derive(Debug, Serialize)]
pub struct SetupCheck {
    /// Setup boundary being observed.
    pub kind: &'static str,
    /// Declared subject or fixed setup step.
    pub subject: String,
    /// `pending`, `ready` or `blocked`.
    pub state: &'static str,
    /// Server-owned human recovery destination.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_action: Option<Action>,
}

/// Progress belongs to the retained fleet resource.
#[derive(Debug, Serialize)]
pub struct SetupProgress {
    /// `checking`, `waiting`, `ready` or `cancelled`.
    pub state: &'static str,
    /// Actual current checks and recovery actions.
    pub checks: Vec<SetupCheck>,
}

pub(super) fn current(row: &Stored, review: &Review) -> SetupProgress {
    let mut checks: Vec<_> = review.readiness.iter().map(observed).collect();
    if review.review_digest != row.accepted.review_digest {
        checks.push(SetupCheck {
            kind: KIND_POLICY,
            subject: "review_changed".to_owned(),
            state: CHECK_BLOCKED,
            next_action: Some(crate::review::review_action(&row.accepted.context)),
        });
    }
    checks.push(SetupCheck {
        kind: "stream",
        subject: "runtime_queue".to_owned(),
        state: if row.setup_stream_ready == Some(true) {
            CHECK_READY
        } else {
            CHECK_PENDING
        },
        next_action: None,
    });
    checks.push(SetupCheck {
        kind: "activation",
        subject: "fleet".to_owned(),
        state: if row.status == ACTIVE {
            CHECK_READY
        } else {
            CHECK_PENDING
        },
        next_action: None,
    });
    SetupProgress {
        state: state(row, &checks),
        checks,
    }
}

/// One review readiness check, reported in setup's vocabulary.
fn observed(check: &Check) -> SetupCheck {
    SetupCheck {
        kind: kind(check.kind),
        subject: check.subject.clone(),
        state: match check.state {
            CheckState::Ready => CHECK_READY,
            CheckState::Missing => CHECK_PENDING,
            CheckState::Blocked => CHECK_BLOCKED,
        },
        next_action: check.next_action.clone(),
    }
}

/// The whole setup's state: the fleet's lifecycle decides first, then whether
/// any check still holds it.
fn state(row: &Stored, checks: &[SetupCheck]) -> &'static str {
    if row.status == KILLED {
        SETUP_CANCELLED
    } else if row.status == ACTIVE {
        SETUP_READY
    } else if checks.iter().any(|check| check.state != CHECK_READY) {
        SETUP_WAITING
    } else {
        SETUP_CHECKING
    }
}

fn kind(kind: CheckKind) -> &'static str {
    match kind {
        CheckKind::Definition => "source",
        CheckKind::Policy => KIND_POLICY,
        CheckKind::Model => "model",
        CheckKind::Wallet => "wallet",
        CheckKind::Credential => "credentials",
        CheckKind::Connection => "connections",
        CheckKind::Grant => "grants",
    }
}

pub(super) fn ready(row: &Stored, review: &Review) -> bool {
    row.accepted.review_digest == review.review_digest
        && review
            .readiness
            .iter()
            .all(|check| check.state == CheckState::Ready)
}
