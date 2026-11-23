//! Private draft conflicts, whose remedies differ from runtime failures.

use super::ErrorCode;

/// Missing or inaccessible private/shared setup resource.
pub const SETUP_NOT_FOUND: ErrorCode = ErrorCode::declare("UZ-SETUP-001");
/// Lifecycle or queued work prevents this mutation.
pub const SETUP_STATE: ErrorCode = ErrorCode::declare("UZ-SETUP-002");
/// The optimistic resource version changed.
pub const SETUP_STALE: ErrorCode = ErrorCode::declare("UZ-SETUP-003");
/// A mutation omitted its required version.
pub const SETUP_PRECONDITION: ErrorCode = ErrorCode::declare("UZ-SETUP-004");
/// A previously accepted key carried different inputs or another actor.
pub const SETUP_REPLAY: ErrorCode = ErrorCode::declare("UZ-SETUP-005");
/// Effective setup or resolved authority differs from the reviewed version.
pub const SETUP_REVIEW: ErrorCode = ErrorCode::declare("UZ-SETUP-006");
/// The owner wallet cannot cover the outstanding setup bounds.
pub const SETUP_BALANCE: ErrorCode = ErrorCode::declare("UZ-SETUP-007");
/// The selected owner model is missing or unsupported by the setup transport.
pub const SETUP_MODEL: ErrorCode = ErrorCode::declare("UZ-SETUP-008");
/// Effective setup policy requires a fresh review.
pub const SETUP_POLICY: ErrorCode = ErrorCode::declare("UZ-SETUP-009");
/// The assembled request exceeds a tokenizer or context-window bound.
pub const SETUP_TOKEN: ErrorCode = ErrorCode::declare("UZ-SETUP-010");
/// A cursor names another private subject or query.
pub const SETUP_CURSOR: ErrorCode = ErrorCode::declare("UZ-SETUP-011");
/// An accepted operation points at a purged resource within its tombstone period.
pub const SETUP_GONE: ErrorCode = ErrorCode::declare("UZ-SETUP-013");
/// The provider failed while returning authoritative usage.
pub const SETUP_PROVIDER: ErrorCode = ErrorCode::declare("UZ-SETUP-014");
/// Dispatch or billing remains unresolved; its bound must be retained.
pub const SETUP_UNKNOWN: ErrorCode = ErrorCode::declare("UZ-SETUP-015");
