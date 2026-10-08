//! The sentence each failure tells a caller, named so a suite can assert one
//! without respelling it.
//!
//! Every string is re-exported from `afd_core::error`, the one place each is
//! spelled. A provider's delivery log shows these to an operator debugging an
//! integration, and two crates answering one incident with different prose
//! would read as two different bugs.

/// The detail for a database that cannot be reached.
pub use afd_core::error::DETAIL_DATABASE_UNAVAILABLE as DATABASE_UNAVAILABLE;

/// The detail for a database that answered with an error.
pub use afd_core::error::DETAIL_DATABASE_ERROR as DATABASE_ERROR;

/// The detail for an operation that failed for any other reason.
///
/// One sentence for the vault, the queue and an unreadable stored document
/// alike — see [`super::Error::answer`] on why naming which of them failed
/// would tell a sender about this deployment's state.
pub use afd_core::error::DETAIL_OPERATION_FAILED as OPERATION_FAILED;
