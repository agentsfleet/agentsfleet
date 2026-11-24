//! Acceptance policy a deployment configures; unset, a knob limits nothing.

mod defaults;
mod new_work;

pub use defaults::DurableTaskDefaults;

use af_wire::durable_task::DurableTaskLimits;
use garde::Validate;
use serde::Deserialize;
use std::time::Duration;

/// Deployment limits for new tasks and their statements.
#[derive(Debug, Clone, Copy, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub struct DurableTaskSettings {
    /// Concurrent unfinished objectives accepted per fleet.
    #[garde(range(min = 1))]
    pub max_unfinished_per_fleet: u32,
    /// Accepted inputs recovered beyond the saved incorporation cursor per read.
    #[garde(range(min = 1))]
    pub max_inputs_per_safe_boundary: usize,
    /// Binding items accepted from one human message.
    #[garde(range(min = 1))]
    pub max_actions_per_message: usize,
    /// Original source bytes retained as one new objective.
    #[garde(range(min = 1))]
    pub max_objective_bytes: usize,
    /// UTF-8 bytes in a new task's compact label.
    #[garde(range(min = 1))]
    pub max_title_bytes: usize,
    /// Completion statements a task is judged against.
    #[garde(range(min = 1))]
    pub max_completion_questions: usize,
    /// UTF-8 bytes in each completion statement's question text.
    #[garde(range(min = 1))]
    pub max_question_text_bytes: usize,
    /// Bytes in the authored question and its criteria together.
    #[garde(range(min = 1))]
    pub max_authored_question_bytes: usize,
    /// Allowances saved on new objectives only.
    #[garde(dive)]
    pub defaults: DurableTaskDefaults,
}

impl Default for DurableTaskSettings {
    fn default() -> Self {
        Self {
            max_unfinished_per_fleet: u32::MAX,
            max_inputs_per_safe_boundary: usize::MAX,
            max_actions_per_message: usize::MAX,
            max_objective_bytes: usize::MAX,
            max_title_bytes: usize::MAX,
            max_completion_questions: usize::MAX,
            max_question_text_bytes: usize::MAX,
            max_authored_question_bytes: usize::MAX,
            defaults: DurableTaskDefaults::default(),
        }
    }
}

/// Checked deployment limits with the compiled defaults for library consumers.
#[derive(Debug, Default, Clone, Copy)]
pub struct DurableTaskPolicy {
    settings: DurableTaskSettings,
}

impl DurableTaskPolicy {
    /// Copies settings whose bounds were checked before startup effects.
    #[must_use]
    pub fn new(settings: &garde::Valid<DurableTaskSettings>) -> Self {
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

    pub(super) const fn limits(self) -> DurableTaskLimits {
        self.settings.defaults.snapshot()
    }
}

/// How long a durable task's slices, operations and checkpoints live, and how
/// many checkpoints it keeps.
///
/// Each default is the number the task plane ran with before it was a setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DurableTaskTimes {
    /// A task runs in slices of this length; its funds are checked before each.
    pub slice: Duration,
    /// How long one task operation may take.
    pub operation_timeout: Duration,
    /// A finished task's records are kept this long.
    pub terminal_retention: Duration,
    /// Newest checkpoints kept per task.
    pub keep_generations: u32,
    /// How long a runner's presigned upload link stays valid.
    pub upload_link_ttl: Duration,
    /// How long a runner's presigned download link stays valid.
    pub download_link_ttl: Duration,
    /// How long a checkpoint's uploads or restore may run in total.
    pub transfer_window: Duration,
    /// A checkpoint being restored cannot be deleted during this window.
    pub restore_pin: Duration,
    /// An upload never committed is deletable after this.
    pub orphan_after: Duration,
    /// Objects deleted per storage call.
    pub delete_batch_size: u32,
}

impl Default for DurableTaskTimes {
    fn default() -> Self {
        Self {
            slice: super::run::SLICE,
            operation_timeout: super::decision::OPERATION_TIMEOUT,
            terminal_retention: super::checkpoint::TERMINAL,
            keep_generations: super::checkpoint::LIVE_GENERATIONS,
            upload_link_ttl: super::checkpoint::LINK,
            download_link_ttl: super::checkpoint::LINK,
            transfer_window: super::checkpoint::TRANSFER,
            restore_pin: super::checkpoint::PIN,
            orphan_after: super::checkpoint::ORPHAN,
            delete_batch_size: super::checkpoint::GC_BATCH,
        }
    }
}

impl DurableTaskTimes {
    pub(crate) fn slice_seconds(&self) -> i32 {
        crate::run::limits::seconds(self.slice)
    }

    pub(crate) fn terminal_seconds(&self) -> i32 {
        crate::run::limits::seconds(self.terminal_retention)
    }

    pub(crate) fn transfer_seconds(&self) -> i32 {
        crate::run::limits::seconds(self.transfer_window)
    }

    pub(crate) fn pin_seconds(&self) -> i32 {
        crate::run::limits::seconds(self.restore_pin)
    }

    pub(crate) fn orphan_seconds(&self) -> i32 {
        crate::run::limits::seconds(self.orphan_after)
    }

    pub(crate) fn live_generations(&self) -> i64 {
        i64::from(self.keep_generations)
    }

    pub(crate) fn gc_batch(&self) -> i64 {
        i64::from(self.delete_batch_size)
    }
}

#[cfg(test)]
mod times_tests {
    use super::DurableTaskTimes;
    use std::time::Duration;

    #[test]
    fn defaults_are_the_numbers_the_durable_task_plane_ran_with() {
        let times = DurableTaskTimes::default();
        assert_eq!(times.slice_seconds(), 600);
        assert_eq!(times.operation_timeout, Duration::from_secs(30));
        assert_eq!(times.terminal_seconds(), 7 * 24 * 60 * 60);
        assert_eq!(times.live_generations(), 3);
        assert_eq!(times.transfer_seconds(), 15 * 60);
        assert_eq!(times.upload_link_ttl, Duration::from_mins(5));
        assert_eq!(times.download_link_ttl, Duration::from_mins(5));
        assert_eq!(times.pin_seconds(), 30 * 60);
        assert_eq!(times.orphan_seconds(), 60 * 60);
        assert_eq!(times.gc_batch(), 256);
    }

    #[test]
    fn a_configured_value_is_what_a_statement_binds() {
        let times = DurableTaskTimes {
            slice: Duration::from_secs(90),
            keep_generations: 5,
            delete_batch_size: 32,
            orphan_after: Duration::from_mins(2),
            ..DurableTaskTimes::default()
        };
        assert_eq!(times.slice_seconds(), 90);
        assert_eq!(times.live_generations(), 5);
        assert_eq!(times.gc_batch(), 32);
        assert_eq!(times.orphan_seconds(), 120);
    }
}
