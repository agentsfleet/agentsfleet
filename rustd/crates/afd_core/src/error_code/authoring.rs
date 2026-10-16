//! Private draft conflicts, whose remedies differ from runtime failures.

use super::ErrorCode;

/// Missing or inaccessible private/shared setup resource.
pub const AUTHORING_NOT_FOUND: ErrorCode = ErrorCode::declare("UZ-AUTHOR-001");
/// Lifecycle or queued work prevents this mutation.
pub const AUTHORING_STATE: ErrorCode = ErrorCode::declare("UZ-AUTHOR-002");
/// The optimistic resource version changed.
pub const AUTHORING_STALE: ErrorCode = ErrorCode::declare("UZ-AUTHOR-003");
/// A mutation omitted its required version.
pub const AUTHORING_PRECONDITION: ErrorCode = ErrorCode::declare("UZ-AUTHOR-004");
/// A previously accepted key carried different inputs or another actor.
pub const AUTHORING_REPLAY: ErrorCode = ErrorCode::declare("UZ-AUTHOR-005");
/// Effective setup or resolved authority differs from the reviewed version.
pub const AUTHORING_REVIEW: ErrorCode = ErrorCode::declare("UZ-AUTHOR-006");
/// The owner wallet cannot cover the outstanding authoring bounds.
pub const AUTHORING_BALANCE: ErrorCode = ErrorCode::declare("UZ-AUTHOR-007");
/// The selected owner model is missing or unsupported by the authoring transport.
pub const AUTHORING_MODEL: ErrorCode = ErrorCode::declare("UZ-AUTHOR-008");
/// Effective setup policy requires a fresh review.
pub const AUTHORING_POLICY: ErrorCode = ErrorCode::declare("UZ-AUTHOR-009");
/// The assembled request exceeds a tokenizer or context-window bound.
pub const AUTHORING_TOKEN: ErrorCode = ErrorCode::declare("UZ-AUTHOR-010");
/// A cursor names another private subject or query.
pub const AUTHORING_CURSOR: ErrorCode = ErrorCode::declare("UZ-AUTHOR-011");
/// An expired, cancelled or superseded attempt cannot publish private output.
pub const AUTHORING_FENCE: ErrorCode = ErrorCode::declare("UZ-AUTHOR-012");
/// An accepted operation points at a purged resource within its tombstone period.
pub const AUTHORING_GONE: ErrorCode = ErrorCode::declare("UZ-AUTHOR-013");
/// The provider failed while returning authoritative usage.
pub const AUTHORING_PROVIDER: ErrorCode = ErrorCode::declare("UZ-AUTHOR-014");
/// Dispatch or billing remains unresolved; its bound must be retained.
pub const AUTHORING_UNKNOWN: ErrorCode = ErrorCode::declare("UZ-AUTHOR-015");
/// A timed-out attempt with resolved provider accounting.
pub const AUTHORING_TIMEOUT: ErrorCode = ErrorCode::declare("UZ-AUTHOR-016");
