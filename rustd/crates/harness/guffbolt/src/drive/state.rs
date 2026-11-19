//! Logical progress that must accompany a recovered conversation.

use serde::{Deserialize, Serialize};

/// Accepted answers and completed turns across execution attempts.
///
/// Provider calls keep their attempt-local number: the replacement still runs
/// its first-call reconciliation even when this logical state has prior turns.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DriveState {
    pub(super) answers: Vec<String>,
    completed_turns: u64,
}

impl DriveState {
    /// Every accepted answer segment, in its original order.
    #[must_use]
    pub fn answers(&self) -> &[String] {
        &self.answers
    }

    /// Applies the replacement's current text policy to retained answer segments.
    pub fn map_answers(&mut self, transform: impl Fn(&str) -> String) {
        for answer in &mut self.answers {
            *answer = transform(answer);
        }
    }

    /// Turns remembered before this boundary, including complete tool groups.
    #[must_use]
    pub const fn completed_turns(&self) -> u64 {
        self.completed_turns
    }

    pub(super) const fn completed(&mut self) {
        self.completed_turns = self.completed_turns.saturating_add(1);
    }
}
