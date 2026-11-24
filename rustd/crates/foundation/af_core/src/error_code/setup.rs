//! A fleet's create, edit and replay conflicts, whose remedies differ from runtime failures.

use super::ErrorCode;

/// Lifecycle or queued work prevents this mutation.
pub const SETUP_STATE: ErrorCode = ErrorCode::declare("UZ-SETUP-002");
/// The optimistic resource version changed.
pub const SETUP_STALE: ErrorCode = ErrorCode::declare("UZ-SETUP-003");
/// A mutation omitted its required version.
pub const SETUP_PRECONDITION: ErrorCode = ErrorCode::declare("UZ-SETUP-004");
/// A previously accepted key carried different inputs or another actor.
pub const SETUP_REPLAY: ErrorCode = ErrorCode::declare("UZ-SETUP-005");
/// The requested or selected model is missing or ineligible for this workspace.
pub const SETUP_MODEL: ErrorCode = ErrorCode::declare("UZ-SETUP-008");
/// A cursor names another subject or query.
pub const SETUP_CURSOR: ErrorCode = ErrorCode::declare("UZ-SETUP-011");
/// An accepted operation points at a purged resource within its tombstone period.
pub const SETUP_GONE: ErrorCode = ErrorCode::declare("UZ-SETUP-013");
