//! Committed fleet history, independent of runnable triggers and runner frames.

mod schedule;

use std::borrow::Cow;

use serde::Serialize;

pub use schedule::{ScheduleChange, ScheduleFire, ScheduleFireDescription, ScheduleSnapshot};

/// Facts committed with their owning mutation, for the fleet's private history.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FleetActivity<'a> {
    /// An immutable setup revision committed.
    SetupSaved {
        /// The saved decimal revision.
        revision: Cow<'a, str>,
        /// Display provenance, independent of caller authority.
        #[serde(skip_serializing_if = "Option::is_none")]
        origin: Option<Cow<'a, str>>,
    },
    /// The checked setup activation committed.
    SetupReady {
        /// The checked decimal revision.
        revision: Cow<'a, str>,
    },
    /// Access was saved, without claiming provider acceptance.
    AccessSaved {
        /// The credential name, never its value.
        service: Cow<'a, str>,
    },
    /// The fleet's workspace memory permissions changed.
    MemorySharingChanged {
        /// Whether the fleet may read workspace memories.
        read: bool,
        /// Whether it may publish workspace memories.
        publish: bool,
    },
    /// Stored schedule intent changed before external registration completes.
    ScheduleChanged {
        /// The committed kind of change.
        change: ScheduleChange,
        /// Display facts at the mutation's commit.
        #[serde(flatten)]
        schedule: ScheduleSnapshot<'a>,
    },
    /// Active registration was confirmed under the current sync claim.
    ScheduleRegistered {
        /// The confirmed schedule revision.
        #[serde(flatten)]
        schedule: ScheduleSnapshot<'a>,
    },
    /// Upstream registration was removed while the schedule remains stored.
    ScheduleUnregistered {
        /// The paused stored schedule.
        #[serde(flatten)]
        schedule: ScheduleSnapshot<'a>,
    },
    /// An upstream failure and its stored reason committed together.
    ScheduleSyncFailed {
        /// The schedule still requiring synchronization.
        #[serde(flatten)]
        schedule: ScheduleSnapshot<'a>,
        /// The stored safe refusal sentence, without a vendor response body.
        last_error: Cow<'a, str>,
    },
    /// Upstream removal was confirmed and the stored schedule was deleted.
    ScheduleRemoved {
        /// Retained display facts after deletion.
        #[serde(flatten)]
        schedule: ScheduleSnapshot<'a>,
    },
    /// An unregistered one-off missed its instant and was removed.
    ScheduleExpired {
        /// Retained display facts; this does not claim a fire occurred.
        #[serde(flatten)]
        schedule: ScheduleSnapshot<'a>,
    },
}
