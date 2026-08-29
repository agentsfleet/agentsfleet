//! The sentence each failure tells a caller, named so a suite can assert one
//! without respelling it.
//!
//! Every string is byte-identical to the `MSG_*` constant in
//! `errors/error_registry.zig` it mirrors, and to the copy
//! `afd_ingress::error::detail` already carries. Two daemons answering one
//! incident with different prose read as two different bugs to whoever is
//! holding the page.

/// `problem_response.zig`'s `internalDbUnavailable` detail.
pub const DATABASE_UNAVAILABLE: &str = "Database unavailable";

/// `problem_response.zig`'s `internalDbError` detail.
pub const DATABASE_ERROR: &str = "Database error";

/// `problem_response.zig`'s `internalOperationError` detail.
pub const OPERATION_FAILED: &str = "Failed to complete the operation";

/// What a caller is told when the external scheduler did not take the change.
///
/// Deliberately says nothing about WHICH way it failed — see
/// [`super::Error::answer`]. What it does say is the thing a person editing a
/// schedule can act on: the row is saved, the schedule is not yet live upstream,
/// and the reconcile will retry it.
pub const UPSTREAM_UNAVAILABLE: &str =
    "The schedule was saved but is not yet registered with the scheduler. It will be retried.";
