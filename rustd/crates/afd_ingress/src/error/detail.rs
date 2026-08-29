//! The sentence each failure tells a caller, named so a suite can assert one
//! without respelling it.
//!
//! Every string is byte-identical to the `MSG_*` constant in
//! `errors/error_registry.zig` it mirrors, and to the copy `afd_vault::error`
//! already carries. A provider's delivery log shows these to an operator
//! debugging an integration, and two daemons answering one incident with
//! different prose would read as two different bugs.

/// `problem_response.zig`'s `internalDbUnavailable` detail.
pub const DATABASE_UNAVAILABLE: &str = "Database unavailable";

/// `problem_response.zig`'s `internalDbError` detail.
pub const DATABASE_ERROR: &str = "Database error";

/// `problem_response.zig`'s `internalOperationError` detail.
///
/// One sentence for the vault, the queue and an unreadable stored document
/// alike — see [`super::Error::answer`] on why naming which of them failed
/// would tell a sender about this deployment's state.
pub const OPERATION_FAILED: &str = "Failed to complete the operation";
