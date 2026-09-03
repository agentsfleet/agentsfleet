//! The shapes a caller sends, and the three renderings every verb answers through.
//!
//! What lives here is the part no single verb owns. [`read`](super::read) and
//! [`write`](super::write) both render a row through [`view_of`], and all four
//! write verbs land on [`held_or`] or [`rendered`] — so a schedule that answers
//! 409 for a superseded reconcile does it in one place rather than four
//! spellings of the same decision.

use afd_core::error_code;
use afd_cron::{Reconciled, Schedule, validate};
use afd_wire::schedule::View;
use axum::Json;
use axum::response::{IntoResponse as _, Response};
use http::StatusCode;
use serde::Deserialize;

use crate::handler::Refusal;

use super::{DETAIL_HELD, DETAIL_NOT_FOUND};

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

/// One row, rendered.
///
/// A free function rather than an inherent `impl`: [`View`] is `afd_wire`'s
/// type now, and the rendering is this plane's knowledge of its own store.
pub(super) fn view_of(schedule: &Schedule) -> View<'_> {
    View {
        schedule_id: schedule.schedule_id.as_str().into(),
        fleet_id: schedule.fleet_id.as_str().into(),
        cron: schedule.cron.as_str().into(),
        timezone: schedule.timezone.as_str().into(),
        message: schedule.message.as_str().into(),
        status: schedule.desired_status.as_str().into(),
        sync: schedule.sync_status.as_str().into(),
        last_error: schedule.last_error.as_deref().map(Into::into),
        created_at: schedule.created_at,
        updated_at: schedule.updated_at,
    }
}

/// One validation verdict, as a refusal.
pub(super) fn checked(
    verdict: Result<(), validate::Invalid>,
    detail: &'static str,
) -> Result<(), Refusal> {
    verdict.map_err(|_invalid| Refusal::coded(error_code::INVALID_REQUEST, detail))
}

/// A reconcile that may have found no row, rendered.
pub(super) fn held_or(
    reconciled: Option<Reconciled>,
    status: StatusCode,
) -> Result<Response, Refusal> {
    reconciled.map_or_else(
        || {
            Err(Refusal::coded(
                error_code::SCHEDULE_NOT_FOUND,
                DETAIL_NOT_FOUND,
            ))
        },
        |reconciled| rendered(reconciled, status),
    )
}

/// What one reconcile answers.
///
/// A superseded attempt is a 409 and not a 500: another syncer holds the row,
/// which is a real state a caller retries out of rather than an incident.
pub(super) fn rendered(reconciled: Reconciled, status: StatusCode) -> Result<Response, Refusal> {
    match reconciled {
        Reconciled::Synced(schedule) | Reconciled::Failed(schedule) => {
            Ok((status, Json(view_of(&schedule))).into_response())
        }
        Reconciled::Removed => Ok(StatusCode::NO_CONTENT.into_response()),
        Reconciled::Superseded => Err(Refusal::coded(error_code::SCHEDULE_SYNCING, DETAIL_HELD)),
    }
}
