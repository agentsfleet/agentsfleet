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
/// A person may retry after the current schedule sync releases its fence.
pub const SYNC_HELD: &str = "This schedule is being synchronised. Try again in a moment.";

/// Independent fields and inherited overlays cannot be mixed.
pub const INVALID_PATCH: &str = "Use an override for an inherited schedule, or fields for an independent schedule. Restore requires an inherited recipe.";
/// A conditional edit requires the schedule tag from its last read.
pub const TAG_REQUIRED: &str = "Send the current schedule tag in If-Match.";
/// The caller must read the schedule again before replacing its intent.
pub const TAG_STALE: &str = "This schedule changed. Read it again before editing.";
/// Deletion must finish before another intent can be written.
pub const DELETING: &str = "This schedule is being removed.";

/// What a caller is told when the external scheduler did not take the change.
///
/// Deliberately says nothing about WHICH way it failed — see
/// [`super::Error::answer`]. What it does say is the thing a person editing a
/// schedule can act on: the row is saved, the schedule is not yet live upstream,
/// and the reconcile will retry it.
pub const UPSTREAM_UNAVAILABLE: &str =
    "The schedule was saved but is not yet registered with the scheduler. It will be retried.";

/// What a caller is told when a `once` schedule fired and could not yet be
/// retired, because another syncer held its row.
///
/// Names the retry, because a repeat is what heals it: the fire replays as the
/// same event and the retirement runs again.
pub const RETIRE_HELD: &str = "The schedule ran, and is being synchronised, so it is not yet \
     removed. Repeat the request in a moment to finish removing it.";

/// What a caller is told when a schedule fire did not prove it came from the
/// external scheduler.
///
/// One sentence for every reason, byte-identical to the one the fire route
/// answers: which check failed is the operator's to read in the log, never the
/// sender's.
pub const FIRE_UNVERIFIED: &str = "The schedule callback could not be verified.";

/// Why a deployment's API URL could not become its schedule callback: it is
/// not a URL, or is past the cap once the ingress path is on it.
pub const DESTINATION_UNUSABLE: &str = "The configured API URL cannot carry a schedule callback.";

/// Why a deployment's API URL could not become its schedule callback: it
/// carries a query or a fragment, which the scheduler would read as its own.
pub const DESTINATION_NOT_PLAIN_ORIGIN: &str =
    "The configured API URL carries a query or a fragment.";
