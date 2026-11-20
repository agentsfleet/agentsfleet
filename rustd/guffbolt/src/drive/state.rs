//! What a loop keeps of its progress, for an embedder that saves a loop and
//! resumes it later.

use serde::{Deserialize, Serialize};

use super::Mark;
use crate::step::Waiting;
use crate::transcript::{Message, Transcript};

/// The answers a loop accepted and the steps it completed, carried from one
/// run of a conversation to the run that resumes it.
///
/// Model calls are numbered within one run: a resumed loop numbers its calls
/// from 1 again, so an extension's first-call step still runs even when the
/// loop completed steps before. A [`crate::CallRef`]'s step goes on from the
/// completed steps instead, so a resumed loop's calls never reuse an earlier
/// call's reference.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DriveState {
    pub(super) answers: Vec<String>,
    completed_steps: u64,
    /// The question the run waits on; absent from a saved state that waits
    /// on none, so a state saved before a run could wait reads the same.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) waiting: Option<Waiting>,
    /// The newest mark of the input the loop took; absent before any marked
    /// input, so a state saved before input carried marks reads the same.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) newest: Option<Mark>,
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

    /// How many steps the loop remembered, tool groups and answers alike.
    #[must_use]
    pub const fn completed_steps(&self) -> u64 {
        self.completed_steps
    }

    /// The question the run waits on, if any.
    #[must_use]
    pub const fn waiting(&self) -> Option<&Waiting> {
        self.waiting.as_ref()
    }

    /// The newest mark of the input the loop took, if any input was marked.
    #[must_use]
    pub const fn newest(&self) -> Option<&Mark> {
        self.newest.as_ref()
    }

    /// Answers the question the run waits on with `answer`, which the model
    /// reads as the asking call's result on the next model call, and hands
    /// back the question answered; `None`, changing nothing, when the run
    /// waits on none. Drive the loop again afterwards to resume it.
    pub fn resume(
        &mut self,
        transcript: &mut Transcript,
        answer: impl Into<String>,
    ) -> Option<Waiting> {
        let waiting = self.waiting.take()?;
        transcript.push(Message::ToolResult {
            call_id: waiting.call_id.clone(),
            output: answer.into(),
            image: None,
        });
        Some(waiting)
    }

    pub(super) const fn completed(&mut self) {
        self.completed_steps = self.completed_steps.saturating_add(1);
    }
}
