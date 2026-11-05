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
    Problem {
        code: error_code::TASK_DECISION_UNAVAILABLE,
        status: 503,
        title: "Decision unavailable",
        hint: "Check the saved decision profile, confirmed credit and platform-admin provider key. The saved input is retained.",
        user_message: Some(
            "The fleet could not assess this request. Your message has been retained.",
        ),
    },
    Problem {
        code: error_code::TASK_PRECONDITION_FAILED,
        status: 412,
        title: "Task version changed",
        hint: "Read the current task and retry the control with its strong ETag and generation.",
        user_message: Some("This task changed. Refresh its progress before applying your control."),
    },
    Problem {
        code: error_code::TASK_CHECKPOINT_INCOMPATIBLE, status: 409, title: "Checkpoint is incompatible",
        hint: "Retain the prior checkpoint. Restore only a verified compatible complete generation.",
        user_message: Some("Saved work could not be restored safely. The previous checkpoint is retained."),
    },
    Problem {
        code: error_code::TASK_INPUT_AMBIGUOUS, status: 409, title: "Input needs clarification",
        hint: "Retain the original complete message. Ask the person to clarify before changing work.",
        user_message: Some("The fleet needs clarification before changing that work."),
    },
    Problem {
        code: error_code::TASK_STORAGE_UNAVAILABLE, status: 503, title: "Checkpoint storage unavailable",
        hint: "Retry the authorized transfer within its original deadline. Do not mark incomplete work saved.",
        user_message: Some("Work could not be saved to storage yet."),
    },
    Problem {
        code: error_code::TASK_EFFECT_UNCERTAIN, status: 409, title: "Publication needs reconciliation",
        hint: "Read the saved effect. Never repeat an uncertain publication as a new operation.",
        user_message: Some("The publication result is being checked before work continues."),
    },
];
