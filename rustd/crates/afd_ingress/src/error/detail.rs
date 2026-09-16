//! The sentence each failure tells a caller, named so a suite can assert one
//! without respelling it.
//!
//! Every string is byte-identical to the `MSG_*` constant in
//! `errors/error_registry.zig` it mirrors, and to the copy `afd_vault::error`
//! already carries. A provider's delivery log shows these to an operator
//! debugging an integration, and two daemons answering one incident with
//! different prose would read as two different bugs.

/// `problem_response.zig`'s `internalDbUnavailable` detail.
pub use afd_core::error::DETAIL_DATABASE_UNAVAILABLE as DATABASE_UNAVAILABLE;

/// `problem_response.zig`'s `internalDbError` detail.
pub use afd_core::error::DETAIL_DATABASE_ERROR as DATABASE_ERROR;

/// `problem_response.zig`'s `internalOperationError` detail.
///
/// One sentence for the vault, the queue and an unreadable stored document
/// alike — see [`super::Error::answer`] on why naming which of them failed
/// would tell a sender about this deployment's state.
pub use afd_core::error::DETAIL_OPERATION_FAILED as OPERATION_FAILED;
