//! The completion statements a durable task is judged done against.
//!
//! A fleet's model writes them when it starts a task, and a person may edit
//! them; the decision model judges each against the task's evidence. Each is
//! a stable id, a question and both of its probability-of-yes criteria.

mod bounds;
mod raw;
mod resolve;

use std::collections::BTreeMap;

use serde::Serialize;

/// Stable statement-key ceiling; the key's character set is ASCII.
pub const MAX_STATEMENT_ID_BYTES: usize = 64;
/// Probability-of-yes criteria; no separate confidence or progress value.
pub const NOUL_CRITERIA: [&str; 2] = ["true", "false"];

/// Stable statement identity with its question and criteria.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CompletionStatement {
    id: Box<str>,
    #[serde(flatten)]
    question: Question,
}

/// Wording with the exact criterion keys a statement takes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Question {
    question: Box<str>,
    criteria: BTreeMap<Box<str>, Box<str>>,
}

impl CompletionStatement {
    /// Identity a person's edit keeps when it rewords the statement.
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
    /// The wording, without runtime substitution or truncation.
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

#[cfg(test)]
mod tests;
