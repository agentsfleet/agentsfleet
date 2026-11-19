//! Admission policy can narrow protocol ceilings without invalidating saved work.

mod defaults;
mod interpretation;

pub use defaults::TaskDefaults;

use af_fleet_runtime::config::decision::{MAX_COMPLETION_STATEMENTS, MAX_QUESTION_TEXT_BYTES};
use af_wire::task::{self, TaskLimits};
use garde::Validate;
use serde::Deserialize;

/// Deployment limits for new interpretations and objectives.
#[derive(Debug, Clone, Copy, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub struct TaskSettings {
    /// Concurrent unfinished objectives admitted per fleet.
    #[garde(range(min = 1, max = task::UNFINISHED_MAX))]
    pub max_unfinished_per_fleet: u32,
    /// Accepted inputs recovered beyond the saved incorporation cursor per read.
    #[garde(range(min = 1, max = task::INPUT_BATCH_MAX))]
    pub max_inputs_per_safe_boundary: usize,
    /// Complete immutable actions accepted from one human message.
    #[garde(range(min = 1, max = task::ITEMS_MAX))]
    pub max_actions_per_message: usize,
    /// Original source bytes retained as one new objective.
    #[garde(range(min = 1, max = task::OBJECTIVE_MAX_BYTES))]
    pub max_objective_bytes: usize,
    /// UTF-8 bytes in a new interpretation's compact label.
    #[garde(range(min = 1, max = task::TITLE_MAX_BYTES))]
    pub max_title_bytes: usize,
    /// Completion statements frozen with a new objective's accepted setup.
    #[garde(range(min = 1, max = MAX_COMPLETION_STATEMENTS))]
    pub max_completion_questions: usize,
    /// UTF-8 bytes in each new objective's completion question text.
    #[garde(range(min = 1, max = MAX_QUESTION_TEXT_BYTES))]
    pub max_question_text_bytes: usize,
    /// Bytes in the authored question and its criteria together.
    #[garde(range(min = 1, max = task::QUESTION_MAX_BYTES))]
    pub max_authored_question_bytes: usize,
    /// Allowances saved on new objectives only.
    #[garde(dive)]
    pub defaults: TaskDefaults,
}

impl Default for TaskSettings {
    fn default() -> Self {
        Self {
            max_unfinished_per_fleet: task::UNFINISHED_MAX,
            max_inputs_per_safe_boundary: task::INPUT_BATCH_MAX,
            max_actions_per_message: task::ITEMS_MAX,
            max_objective_bytes: task::OBJECTIVE_MAX_BYTES,
            max_title_bytes: task::TITLE_MAX_BYTES,
            max_completion_questions: MAX_COMPLETION_STATEMENTS,
            max_question_text_bytes: MAX_QUESTION_TEXT_BYTES,
            max_authored_question_bytes: task::QUESTION_MAX_BYTES,
            defaults: TaskDefaults::default(),
        }
    }
}

/// Checked deployment limits with the compiled defaults for library consumers.
#[derive(Debug, Default, Clone, Copy)]
pub struct TaskPolicy {
    settings: TaskSettings,
}

impl TaskPolicy {
    /// Copies settings whose bounds were checked before startup effects.
    #[must_use]
    pub fn new(settings: &garde::Valid<TaskSettings>) -> Self {
        Self {
            settings: **settings,
        }
    }

    pub(super) const fn unfinished(self) -> u32 {
        self.settings.max_unfinished_per_fleet
    }

    pub(super) const fn input_batch(self) -> usize {
        self.settings.max_inputs_per_safe_boundary
    }

    pub(super) const fn limits(self) -> TaskLimits {
        self.settings.defaults.snapshot()
    }
}
