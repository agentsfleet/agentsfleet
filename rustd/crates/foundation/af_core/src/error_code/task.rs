//! Durable objective refusals, distinct from the current execution lease.

use super::ErrorCode;

/// No task belongs to the authorized workspace and fleet.
pub const TASK_NOT_FOUND: ErrorCode = ErrorCode::declare("UZ-DT-001");
/// The objective changed or no longer permits the requested transition.
pub const TASK_STALE_STATE: ErrorCode = ErrorCode::declare("UZ-DT-002");
/// An existing interpretation or operation was replayed with different content.
pub const TASK_REPLAY_CONFLICT: ErrorCode = ErrorCode::declare("UZ-DT-003");
/// The requested objective has no accepted native target binding.
pub const TASK_INVALID_TARGET: ErrorCode = ErrorCode::declare("UZ-DT-004");
/// A task allowance or the unfinished-objective capacity was exhausted.
pub const TASK_LIMIT_EXHAUSTED: ErrorCode = ErrorCode::declare("UZ-DT-005");
/// A reviewed decision profile, confirmed funding or its platform key is unavailable.
pub const TASK_DECISION_UNAVAILABLE: ErrorCode = ErrorCode::declare("UZ-DT-006");
/// Ambiguous intent or association requires clarification before changing work.
pub const TASK_INPUT_AMBIGUOUS: ErrorCode = ErrorCode::declare("UZ-DT-007");
/// The retained checkpoint cannot safely restore this task generation.
pub const TASK_CHECKPOINT_INCOMPATIBLE: ErrorCode = ErrorCode::declare("UZ-DT-008");
/// Checkpoint storage or a registered immutable object is unavailable.
pub const TASK_STORAGE_UNAVAILABLE: ErrorCode = ErrorCode::declare("UZ-DT-009");
/// Publication may have happened; reconcile its immutable intent before continuing.
pub const TASK_EFFECT_UNCERTAIN: ErrorCode = ErrorCode::declare("UZ-DT-010");

/// A conditional task mutation named an old resource version.
pub const TASK_PRECONDITION_FAILED: ErrorCode = ErrorCode::declare("UZ-DT-013");
