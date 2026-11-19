//! Checkpoint recovery refusals retain the runner wire status and code.

use super::Problem;
use crate::error_code;

pub(super) const INCOMPATIBLE: Problem = Problem {
    code: error_code::RUN_CHECKPOINT_INCOMPATIBLE,
    status: 409,
    title: "Run checkpoint incompatible",
    hint: "Saved run progress cannot be restored with this setup. Restore the matching setup or start a new run.",
    user_message: None,
};

pub(super) const CONFLICT: Problem = Problem {
    code: error_code::RUN_CHECKPOINT_CONFLICT,
    status: 409,
    title: "Run checkpoint conflict",
    hint: "The saved recovery head changed. Read the current descriptor before starting another save.",
    user_message: None,
};

pub(super) const UNAVAILABLE: Problem = Problem {
    code: error_code::RUN_CHECKPOINT_UNAVAILABLE,
    status: 503,
    title: "Run checkpoint unavailable",
    hint: "The saved recovery point is unavailable. Retry when storage returns; keep the last committed recovery point.",
    user_message: None,
};

pub(super) const LIMIT: Problem = Problem {
    code: error_code::RUN_CHECKPOINT_LIMIT,
    status: 413,
    title: "Run checkpoint limit exceeded",
    hint: "The recovery point exceeds a declared limit. Reduce the working set before saving another checkpoint.",
    user_message: None,
};
