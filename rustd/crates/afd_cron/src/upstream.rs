//! The id a schedule is registered under at the external scheduler.
//!
//! # This daemon names the schedule, and the fire names it back
//!
//! The scheduler signs every fire, but the schedule that fell due rides in a
//! header beside the token. A schedule registered without an id of this
//! daemon's choosing is filed under a random one, so its fire names a schedule
//! no row here is keyed by and is dropped as unknown — every fire, forever. So
//! every push sends [`HEADER_SCHEDULE_ID`] carrying [`upstream_id`], the
//! scheduler files the schedule under it, and the fire path reads the row back
//! with [`schedule_of`]. The scheduler overwrites a schedule pushed again under
//! the same id, so an edit, a resume or a `:sync` leaves one registration
//! behind rather than one more.
//!
//! # Why the id is prefixed rather than the bare row id
//!
//! A `source_key` equal to the row's own id is how the reconciler recognises a
//! schedule the scheduler never held (`service::never_registered`). Filing the
//! schedule under the bare row id would make "registered" and "never
//! registered" the same value, and a one-off resumed after its moment would be
//! retired while its fire was still on the way.

use afd_core::id::Uuid7;

/// The header a push names its schedule id in, and a fire names it back in.
///
/// One name for both directions: the scheduler reads it on a create and sends
/// it on every delivery a schedule makes.
pub const HEADER_SCHEDULE_ID: &str = "upstash-schedule-id";

/// What every schedule id this daemon registers begins with.
const UPSTREAM_ID_PREFIX: &str = "fleet-schedule-";

/// The id `schedule` is registered under at the scheduler.
#[must_use]
pub fn upstream_id(schedule: &Uuid7) -> String {
    format!("{UPSTREAM_ID_PREFIX}{}", schedule.as_str())
}

/// The schedule a fire names, or `None` for an id this daemon never issued.
///
/// A schedule the scheduler filed under an id of its own answers `None`: no
/// row is keyed by it, and guessing one from its shape would let a header
/// pick a schedule the signed token says nothing about.
#[must_use]
pub fn schedule_of(upstream: &str) -> Option<Uuid7> {
    upstream
        .strip_prefix(UPSTREAM_ID_PREFIX)
        .and_then(|row| Uuid7::parse(row).ok())
}

#[cfg(test)]
mod tests;
