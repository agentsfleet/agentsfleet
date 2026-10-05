//! How a schedule write answers, for every plane that writes one.
//!
//! The tenant surface and the runner's schedules verb drive the same store and
//! reconciler, so they answer the same outcomes the same way: a reconciled row
//! is its view, a removed one is a `204`, and a row another syncer holds is a
//! `409` a caller retries out of. One rendering, so a person and a fleet read
//! one schedule identically.

use afd_core::error_code;
use afd_cron::{Reconciled, Refused, validate};
use axum::Json;
use axum::response::{IntoResponse as _, Response};
use http::StatusCode;

use super::Refusal;

/// The refusal a schedule another syncer is holding earns.
///
/// A conflict rather than a not-found, because the row EXISTS and the caller
/// may retry in a moment — the two answers send a caller to different places.
pub const DETAIL_HELD: &str = "This schedule is being synchronised. Try again in a moment.";

/// The `current_state` a schedule another syncer holds names.
pub const STATE_SYNCING: &str = "syncing";

/// The refusal a fleet changing a schedule a person made earns.
pub const DETAIL_NOT_FLEET_OWNED: &str =
    "A fleet can change or delete only the schedules it created; a person created this one.";

/// A schedule's fields, bounded and read, or the refusal the first broken one
/// earns.
///
/// # Errors
/// `UZ-REQ-001` carrying the sentence of the first field that broke.
pub fn checked(fields: validate::Fields<'_>) -> Result<(), Refusal> {
    fields
        .check()
        .map_err(|invalid| Refusal::coded(error_code::INVALID_REQUEST, invalid.detail()))
}

/// A refused create, as the caller reads it.
#[must_use]
pub fn refused(refusal: Refused) -> Refusal {
    match refusal.current_state() {
        Some(state) => Refusal::conflict(refusal.code(), refusal.detail(), state),
        None => Refusal::coded(refusal.code(), refusal.detail()),
    }
}

/// What one reconcile answers.
///
/// A superseded attempt is a 409 and not a 500: another syncer holds the row,
/// which is a real state a caller retries out of rather than an incident.
///
/// # Errors
/// `UZ-SCHED-006` for a superseded reconcile.
pub fn rendered(reconciled: Reconciled, status: StatusCode) -> Result<Response, Refusal> {
    match reconciled {
        Reconciled::Synced(schedule) | Reconciled::Failed(schedule) => {
            Ok((status, Json(schedule.view())).into_response())
        }
        Reconciled::Removed => Ok(StatusCode::NO_CONTENT.into_response()),
        Reconciled::Superseded => Err(Refusal::conflict(
            error_code::SCHEDULE_SYNCING,
            DETAIL_HELD,
            STATE_SYNCING,
        )),
    }
}

/// A reconcile that may have found no row, rendered.
///
/// # Errors
/// `UZ-SCHED-002` for a schedule the fleet does not hold, and what
/// [`rendered`] refuses.
pub fn held_or(reconciled: Option<Reconciled>, status: StatusCode) -> Result<Response, Refusal> {
    reconciled.map_or_else(
        || Err(not_found()),
        |reconciled| rendered(reconciled, status),
    )
}

/// The refusal a fleet earns for a schedule it did not create.
#[must_use]
pub fn not_fleet_owned() -> Refusal {
    Refusal::coded(error_code::SCHEDULE_NOT_FLEET_OWNED, DETAIL_NOT_FLEET_OWNED)
}

/// The refusal a schedule the fleet does not hold earns.
#[must_use]
pub fn not_found() -> Refusal {
    refused(Refused::NoSuchFleet)
}

#[cfg(test)]
#[path = "schedule/tests.rs"]
mod tests;
