//! What a loop keeps of its progress, for an embedder that saves a loop and
//! resumes it later.

use serde::{Deserialize, Serialize};

/// The answers a loop accepted and the turns it completed, carried from one
/// run of a conversation to the run that resumes it.
///
/// Model calls are numbered within one run: a resumed loop numbers its calls
/// from 1 again, so an extension's first-call step still runs even when the
/// loop completed turns before.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DriveState {
    pub(super) answers: Vec<String>,
    completed_turns: u64,
}

impl DriveState {
    /// Every accepted answer, in the order the model gave them.
    #[must_use]
    pub fn answers(&self) -> &[String] {
        &self.answers
    }

    /// Rewrites every accepted answer through `transform`, such as the
    /// masking a resumed run applies to what it carries over.
    pub fn map_answers(&mut self, transform: impl Fn(&str) -> String) {
        for answer in &mut self.answers {
            *answer = transform(answer);
        }
    }

    /// How many turns the loop remembered, tool groups and answers alike.
    #[must_use]
    pub const fn completed_turns(&self) -> u64 {
        self.completed_turns
    }

    pub(super) const fn completed(&mut self) {
        self.completed_turns = self.completed_turns.saturating_add(1);
    }
}
