//! The sentence each failure tells a caller, named so a suite can assert one
//! without respelling it.
//!
//! Every string is byte-identical to the copy `afd_ingress::error::detail`
//! already carries. Two services answering one
//! incident with different prose read as two different bugs to whoever is
//! holding the page.

/// The detail for a database that cannot be reached.
pub use afd_core::error::DETAIL_DATABASE_UNAVAILABLE as DATABASE_UNAVAILABLE;

/// The detail for a database that answered with an error.
pub use afd_core::error::DETAIL_DATABASE_ERROR as DATABASE_ERROR;

/// The detail for an operation that failed for any other reason.
pub use afd_core::error::DETAIL_OPERATION_FAILED as OPERATION_FAILED;

/// What a caller is told when the external scheduler did not take the change.
///
/// Deliberately says nothing about WHICH way it failed — see
/// [`super::Error::answer`]. What it does say is the thing a person editing a
/// schedule can act on: the row is saved, the schedule is not yet live upstream,
/// and the reconcile will retry it.
pub const UPSTREAM_UNAVAILABLE: &str =
    "The schedule was saved but is not yet registered with the scheduler. It will be retried.";
