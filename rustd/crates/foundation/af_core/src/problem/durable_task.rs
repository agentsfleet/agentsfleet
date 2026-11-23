//! Public outcomes of guarded durable-task mutations.

use super::Problem;
use crate::error_code;

pub(super) const DURABLE_TASK: &[Problem] = &[
    Problem {
        code: error_code::DURABLE_TASK_NOT_FOUND,
        status: 404,
        title: "Durable task not found",
        hint: "No durable task belongs to this workspace and fleet. Read the current fleet durable task list.",
        user_message: Some("That durable task is unavailable. Refresh the fleet's work list."),
    },
    Problem {
        code: error_code::DURABLE_TASK_STALE_STATE,
        status: 409,
        title: "Durable task state changed",
        hint: "Read current durable task state before another transition. Terminal durable tasks cannot restart.",
        user_message: Some(
            "The durable task changed. Refresh its current state before continuing.",
        ),
    },
    Problem {
        code: error_code::DURABLE_TASK_REPLAY_CONFLICT,
        status: 409,
        title: "Durable task replay conflicts",
        hint: "The saved operation has different content. Reuse its original immutable interpretation.",
        user_message: Some("This request conflicts with an earlier accepted operation."),
    },
    Problem {
        code: error_code::DURABLE_TASK_INVALID_TARGET,
        status: 409,
        title: "Durable task target is unavailable",
        hint: "Supply an authorized target under the accepted goal and current repository grants.",
        user_message: Some("This durable task needs an authorized target before it can proceed."),
    },
    Problem {
        code: error_code::DURABLE_TASK_LIMIT_EXHAUSTED,
        status: 429,
        title: "Durable task limit reached",
        hint: "An objective's cumulative allowance or the fleet's unfinished-durable-task capacity is exhausted.",
        user_message: Some(
            "This durable task or fleet reached its work limit. Progress has been retained.",
        ),
    },
    Problem {
        code: error_code::DURABLE_TASK_DECISION_UNAVAILABLE,
        status: 503,
        title: "Decision unavailable",
        hint: "Check the saved decision profile, confirmed credit and platform-admin provider key. The saved input is retained.",
        user_message: Some(
            "The fleet could not assess this request. Your message has been retained.",
        ),
    },
    Problem {
        code: error_code::DURABLE_TASK_PRECONDITION_FAILED,
        status: 412,
        title: "Durable task version changed",
        hint: "Read the current durable task and retry the control with its strong ETag and generation.",
        user_message: Some(
            "This durable task changed. Refresh its progress before applying your control.",
        ),
    },
    Problem {
        code: error_code::DURABLE_TASK_CHECKPOINT_INCOMPATIBLE,
        status: 409,
        title: "Checkpoint is incompatible",
        hint: "Retain the prior checkpoint. Restore only a verified compatible complete generation.",
        user_message: Some(
            "Saved work could not be restored safely. The previous checkpoint is retained.",
        ),
    },
    Problem {
        code: error_code::DURABLE_TASK_INPUT_AMBIGUOUS,
        status: 409,
        title: "Input needs clarification",
        hint: "Retain the original complete message. Ask the person to clarify before changing work.",
        user_message: Some("The fleet needs clarification before changing that work."),
    },
    Problem {
        code: error_code::DURABLE_TASK_STORAGE_UNAVAILABLE,
        status: 503,
        title: "Checkpoint storage unavailable",
        hint: "Retry the authorized transfer within its original deadline. Do not mark incomplete work saved.",
        user_message: Some("Work could not be saved to storage yet."),
    },
    Problem {
        code: error_code::DURABLE_TASK_EFFECT_UNCERTAIN,
        status: 409,
        title: "Publication needs reconciliation",
        hint: "Read the saved effect. Never repeat an uncertain publication as a new operation.",
        user_message: Some("The publication result is being checked before work continues."),
    },
];
