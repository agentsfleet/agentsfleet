//! The shapes a caller sends.
//!
//! The renderings every verb answers through are
//! `afd_http::handler::schedule`'s, shared with the runner's schedules verb, so
//! a schedule that answers 409 for a superseded reconcile does it in one place
//! for both surfaces.

use serde::Deserialize;

/// The two segments beyond the workspace that an item template carries.
///
/// A named struct rather than `Path<(String, String)>`: the template carries
/// THREE parameters, and a tuple extractor would take them positionally — which
/// binds correctly only for as long as nobody reorders the path. The workspace
/// is not here; [`crate::auth::WorkspaceContext`] carries the one the
/// ownership layer proved.
#[derive(Debug, Deserialize)]
pub(crate) struct SchedulePath {
    /// The fleet named in the path, still text.
    pub(crate) fleet_id: String,
    /// The schedule named in the path, still text.
    pub(crate) schedule_id: String,
}

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
