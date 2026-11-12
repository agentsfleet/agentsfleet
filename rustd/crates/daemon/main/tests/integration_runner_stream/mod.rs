//! Runner lifecycle operations cross the native listener and the production router.

/// What a loaded lane gets on top of the carrier's recovery budget before a
/// wall-clock wait gives up: scheduling slack, not part of the design budget.
pub(crate) const RECOVERY_SLACK: std::time::Duration = std::time::Duration::from_secs(2);

#[cfg(feature = "test-util")]
mod authoring;
mod authority;
mod heartbeat;
mod offers;

mod drain;
mod fairness;
mod lifecycle;
#[cfg(feature = "test-util")]
pub(crate) mod raw;
mod renewal;
mod replay;

mod recovery;

mod lifecycle_selection;

mod usage_replay;

mod measurements;
mod population;

mod isolation;
mod reconciliation;
