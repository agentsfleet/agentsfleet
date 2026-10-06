//! The shapes a caller sends.
//!
//! The renderings every verb answers through are
//! `afd_http::handler::schedule`'s, shared with the runner's schedules verb, so
//! a schedule that answers 409 for a superseded reconcile does it in one place
//! for both surfaces.

use serde::Deserialize;

/// What a caller sends to create a schedule.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(as = ScheduleWrite))]
#[derive(Debug, Deserialize)]
pub(super) struct Create {
    /// The expression it fires on.
    pub(super) cron: String,
    /// The zone that expression is read in. Absent means UTC.
    // A schedule with no stated zone is not an error, it is one written by
    // somebody who did not think about zones, and UTC (the default in
    // `afd_cron::model::DEFAULT_TIMEZONE`) is the answer that surprises them
    // least.
    pub(super) timezone: Option<String>,
    /// What the fleet is asked to do when it fires.
    pub(super) message: String,
}

/// What a caller sends to change one.
///
/// Every field optional, and an absent one is left alone — see
/// [`afd_cron::Change`] on why a partial edit is not a whole replacement.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(as = SchedulePatch))]
#[derive(Debug, Deserialize)]
pub(super) struct Patch {
    /// A new expression.
    pub(super) cron: Option<String>,
    /// A new zone.
    pub(super) timezone: Option<String>,
    /// A new message.
    pub(super) message: Option<String>,
    /// Whether it should be firing.
    pub(super) paused: Option<bool>,
}
