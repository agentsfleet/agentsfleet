//! Public outcomes of guarded durable-task mutations.

use super::Problem;
use crate::error_code;

pub(super) const TASK: &[Problem] = &[
    Problem {
        code: error_code::TASK_NOT_FOUND,
        status: 404,
        title: "Task not found",
        hint: "No task belongs to this workspace and fleet. Read the current fleet task list.",
        user_message: Some("That task is unavailable. Refresh the fleet's work list."),
    },
    Problem {
        code: error_code::TASK_STALE_STATE,
        status: 409,
        title: "Task state changed",
        hint: "Read current task state before another transition. Terminal tasks cannot restart.",
        user_message: Some("The task changed. Refresh its current state before continuing."),
    },
    Problem {
        code: error_code::TASK_REPLAY_CONFLICT,
        status: 409,
        title: "Task replay conflicts",
        hint: "The saved operation has different content. Reuse its original immutable interpretation.",
        user_message: Some("This request conflicts with an earlier accepted operation."),
    },
    Problem {
        code: error_code::TASK_INVALID_TARGET,
        status: 409,
        title: "Task target is unavailable",
        hint: "Supply an authorized target under the accepted goal and current repository grants.",
        user_message: Some("This task needs an authorized target before it can proceed."),
    },
    Problem {
        code: error_code::TASK_LIMIT_EXHAUSTED,
        status: 429,
        title: "Task limit reached",
        hint: "An objective's cumulative allowance or the fleet's unfinished-task capacity is exhausted.",
        user_message: Some(
            "This task or fleet reached its work limit. Progress has been retained.",
        ),
    },
];
