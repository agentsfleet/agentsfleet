//! One row, as the value type.
//!
//! Split from [`super`] because it is the file every statement's correctness
//! rests on: the reads are positional, against [`crate::sql`]'s shared column
//! macro, and a field read at the wrong index is a schedule that registers
//! upstream under whatever the neighbouring column held. Keeping the decoder
//! alone makes it the thing a reviewer checks against the macro, rather than a
//! tail somebody scrolls past.

use afd_core::id::Uuid7;
use sqlx::Row as _;

use crate::error::{
    self, COLUMN_DESIRED_STATUS, COLUMN_FLEET, COLUMN_ID, COLUMN_SOURCE, COLUMN_SYNC_STATUS, Result,
};
use crate::model::{DesiredStatus, Schedule, Source, SyncStatus};

/// The context a failed read reports under.
const CONTEXT_READ: &str = "read a schedule";

/// One row, as the value type.
pub(super) fn decode(row: &sqlx::postgres::PgRow) -> Result<Schedule> {
    let unreadable = error::query(CONTEXT_READ);
    let schedule_id: String = row.try_get(0).map_err(&unreadable)?;
    let fleet_id: String = row.try_get(1).map_err(&unreadable)?;
    let source: String = row.try_get(2).map_err(&unreadable)?;
    let desired_status: String = row.try_get(7).map_err(&unreadable)?;
    let sync_status: String = row.try_get(8).map_err(&unreadable)?;

    Ok(Schedule {
        schedule_id: Uuid7::parse(&schedule_id)
            .map_err(|_shape| error::row_unreadable(COLUMN_ID))?,
        fleet_id: Uuid7::parse(&fleet_id).map_err(|_shape| error::row_unreadable(COLUMN_FLEET))?,
        source: Source::parse(&source).ok_or_else(|| error::row_unreadable(COLUMN_SOURCE))?,
        source_key: row.try_get(3).map_err(&unreadable)?,
        cron: row.try_get(4).map_err(&unreadable)?,
        timezone: row.try_get(5).map_err(&unreadable)?,
        message: row.try_get(6).map_err(&unreadable)?,
        desired_status: DesiredStatus::parse(&desired_status)
            .ok_or_else(|| error::row_unreadable(COLUMN_DESIRED_STATUS))?,
        sync_status: SyncStatus::parse(&sync_status)
            .ok_or_else(|| error::row_unreadable(COLUMN_SYNC_STATUS))?,
        generation: row.try_get(9).map_err(&unreadable)?,
        sync_token: row.try_get(10).map_err(&unreadable)?,
        sync_lease_until: row.try_get(11).map_err(&unreadable)?,
        last_error: row.try_get(12).map_err(&unreadable)?,
        created_at: row.try_get(13).map_err(&unreadable)?,
        updated_at: row.try_get(14).map_err(&unreadable)?,
    })
}
