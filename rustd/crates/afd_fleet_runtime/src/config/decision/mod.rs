//! Owner-reviewed questions belong to the existing immutable fleet setup.

mod bounds;
pub(crate) mod raw;
mod resolve;
#[cfg(feature = "openapi")]
pub(crate) mod schema;

use std::collections::BTreeMap;

use serde::Serialize;

/// Most accepted semantic completion statements in a task goal.
pub const MAX_COMPLETION_STATEMENTS: usize = 6;
/// The existing typed decision protocol's UTF-8 text ceiling.
pub const MAX_QUESTION_TEXT_BYTES: usize = 512;
/// Stable statement-key ceiling; the key's character set is ASCII.
pub const MAX_STATEMENT_ID_BYTES: usize = 64;
#[cfg(feature = "openapi")]
pub(super) const STATEMENT_ID_PATTERN: &str = r"^[A-Za-z0-9_.-]+$";
/// Fixed routing answers; owners edit their meaning, not their action identity.
pub const INTENT_OPTIONS: [&str; 6] = [
    "conversation",
    "status",
    "refresh",
    "steer",
    "new_work",
    "clarify",
];
/// Association criteria apply to native candidate aliases supplied at dispatch.
pub const ASSOCIATION_CRITERIA: [&str; 3] = ["matching_task", "new_work", "unclear"];
/// Probability-of-yes criteria; no separate confidence or progress value.
pub const NOUL_CRITERIA: [&str; 2] = ["true", "false"];
/// Fixed goal transitions remain proposals subject to native evidence checks.
pub const NEXT_ACTION_OPTIONS: [&str; 4] = ["CONTINUE", "WAIT", "DONE", "BLOCKED"];

/// A complete policy proved by the same parser as the rest of fleet setup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DecisionPolicy {
    routing: RoutingQuestions,
    completion: CompletionQuestions,
}

/// The three reviewed questions used to interpret an admitted human message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RoutingQuestions {
    intent: Question,
    association: Question,
    multiple_requests: Question,
}

/// Required statements and next-action criteria accepted by a task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CompletionQuestions {
    statements: Box<[CompletionStatement]>,
    next_action: Question,
}

/// Stable statement identity with its owner-reviewed question and criteria.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CompletionStatement {
    id: Box<str>,
    #[serde(flatten)]
    question: Question,
}

/// Immutable wording with the exact criterion keys allowed for its native role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Question {
    question: Box<str>,
    criteria: BTreeMap<Box<str>, Box<str>>,
}

impl DecisionPolicy {
    /// Questions for the accepted message, never repository-supplied text.
    #[must_use]
    pub const fn routing(&self) -> &RoutingQuestions {
        &self.routing
    }

    /// Goal questions to freeze with a task's accepted setup revision.
    #[must_use]
    pub const fn completion(&self) -> &CompletionQuestions {
        &self.completion
    }
}

impl RoutingQuestions {
    /// Conversation, status, refresh, steering, new work or clarification.
    #[must_use]
    pub const fn intent(&self) -> &Question {
        &self.intent
    }

    /// Which trusted candidate matches, or whether work is new or unclear.
    #[must_use]
    pub const fn association(&self) -> &Question {
        &self.association
    }

    /// Whether the complete newest message contains independent requests.
    #[must_use]
    pub const fn multiple_requests(&self) -> &Question {
        &self.multiple_requests
    }
}

impl CompletionQuestions {
    /// Every required semantic statement, in the owner's reviewed order.
    #[must_use]
    pub fn statements(&self) -> &[CompletionStatement] {
        &self.statements
    }

    /// The proposal to continue, wait, finish or remain blocked.
    #[must_use]
    pub const fn next_action(&self) -> &Question {
        &self.next_action
    }
}

impl CompletionStatement {
    /// Identity retained across explicitly adopted setup revisions.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Question wording with both probability-of-yes criteria.
    #[must_use]
    pub const fn question(&self) -> &Question {
        &self.question
    }
}

impl Question {
    /// Owner-reviewed wording, without runtime substitution or truncation.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.question
    }

    /// Read-only criterion lookup; asking for another role's key returns none.
    #[must_use]
    pub fn criterion(&self, key: &str) -> Option<&str> {
        self.criteria.get(key).map(AsRef::as_ref)
    }

    /// The complete criteria in stable key order for display and rendering.
    pub fn criteria(&self) -> impl ExactSizeIterator<Item = (&str, &str)> {
        self.criteria
            .iter()
            .map(|(key, value)| (key.as_ref(), value.as_ref()))
    }
}
