//! The sentence each failure tells a caller, named so a suite can assert one
//! without respelling it.
//!
//! The memory operator surface's sentences are part of the answer a client
//! reads, so each is spelled once here and changing one is a visible change.

/// The detail for a database that cannot be reached.
pub use afd_core::error::DETAIL_DATABASE_UNAVAILABLE as DATABASE_UNAVAILABLE;

/// The detail for a database that answered with an error.
pub use afd_core::error::DETAIL_DATABASE_ERROR as DATABASE_ERROR;

/// The detail for an operation that failed for any other reason.
pub use afd_core::error::DETAIL_OPERATION_FAILED as OPERATION_FAILED;

/// The fleet does not exist, or is not in the caller's workspace — one answer
/// for both, so a probe learns nothing. Lower-case, unlike [`ENTRY_NOT_FOUND`].
pub const FLEET_NOT_FOUND: &str = "fleet not found";

/// An operator statement could not take the memory role (`SET LOCAL ROLE`).
pub const ROLE_SWITCH: &str = "memory backend role switch failed";

/// A refused recent or category read.
pub const LIST_FAILED: &str = "memory list failed";

/// A refused `?query=` statement.
pub const SEARCH_FAILED: &str = "memory search failed";

/// A refused forget.
pub const FORGET_FAILED: &str = "memory forget failed";

/// A forget naming a key the fleet holds no entry under.
pub const ENTRY_NOT_FOUND: &str = "No memory entry with that key";

/// A call that reached the workspace while its memory was being copied to
/// another store; retrying after the copy finishes succeeds.
pub const MOVING: &str = "memory is moving to another store; try again shortly";

/// A flip that did not switch because a write made while it copied failed on
/// the store being filled; the workspace stays where it was, holding that
/// write, and the flip can be run again.
pub const MISSED_WRITE: &str =
    "a write did not reach the store memory was moving to; memory stayed where it was";

/// A stand-in store's refusal.
#[cfg(feature = "test-util")]
pub const STORE_REFUSED: &str = "the memory store refused the call";
