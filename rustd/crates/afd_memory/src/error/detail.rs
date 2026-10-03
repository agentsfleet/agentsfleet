//! The sentence each failure tells a caller, named so a suite can assert one
//! without respelling it.
//!
//! The memory operator surface's are pinned to `memory/handler.zig` and its
//! `helpers.zig`: a client comparing bytes across the two daemons sees no
//! difference.

/// The detail for a database that cannot be reached.
pub use afd_core::error::DETAIL_DATABASE_UNAVAILABLE as DATABASE_UNAVAILABLE;

/// The detail for a database that answered with an error.
pub use afd_core::error::DETAIL_DATABASE_ERROR as DATABASE_ERROR;

/// The detail for an operation that failed for any other reason.
pub use afd_core::error::DETAIL_OPERATION_FAILED as OPERATION_FAILED;

/// `helpers.zig`'s `S_AGENTSFLEET_NOT_FOUND`, lower-case as the Zig spells it.
pub const FLEET_NOT_FOUND: &str = "fleet not found";

/// `handler.zig`'s `S_MEMORY_BACKEND_ROLE_SWITCH_FAILED`.
pub const ROLE_SWITCH: &str = "memory backend role switch failed";

/// `handler.zig`'s `S_MEMORY_LIST_FAILED` — the recent and category reads.
pub const LIST_FAILED: &str = "memory list failed";

/// `handler.zig`'s sentence for a refused `?query=` statement.
pub const SEARCH_FAILED: &str = "memory search failed";

/// `handler.zig`'s sentence for a refused forget.
pub const FORGET_FAILED: &str = "memory forget failed";

/// `handler.zig`'s `S_MEMORY_ENTRY_NOT_FOUND`.
pub const ENTRY_NOT_FOUND: &str = "No memory entry with that key";

/// A grant read or write that the database refused.
pub const ACCESS_FAILED: &str = "memory access could not be read or changed";

/// A call that reached the workspace while its memory was being copied to
/// another store; retrying after the copy finishes succeeds.
pub const MOVING: &str = "memory is moving to another store; try again shortly";

/// A stand-in store's refusal.
#[cfg(feature = "test-util")]
pub const STORE_REFUSED: &str = "the memory store refused the call";
