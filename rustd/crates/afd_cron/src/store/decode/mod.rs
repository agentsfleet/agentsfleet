//! One row, as the value type.
//!
//! Split from [`super`] because it is where every statement's correctness lands.
//! Columns are read BY NAME through a derived [`ScheduleRow`]: a positional
//! decoder makes the order in the statement load-bearing, and two `TEXT`
//! columns transposed there would read silently swapped —
//! `provider_schedule_id` into `cron_expression` is a schedule that registers
//! upstream under a cron expression as its key. A name cannot be transposed,
//! and a name that is wrong fails loudly on the first row.
//!
//! `FromRow` on [`Schedule`] itself, delegating to the derived row, so every
//! statement decodes through `query_as` and folds each column read into the
//! one failure sqlx already reports for the query.

use afd_core::id::Uuid7;
use jiff::Timestamp;
use sqlx::FromRow;
use sqlx::postgres::PgRow;

use crate::model::{DesiredStatus, Schedule, Source, SyncStatus};

/// See [`ScheduleRow::id`].
const COL_ID: &str = "id";
/// See [`ScheduleRow::fleet_id`].
const COL_FLEET: &str = "fleet_id";
/// See [`ScheduleRow::source`].
const COL_SOURCE: &str = "source";
/// See [`ScheduleRow::desired_status`].
const COL_DESIRED_STATUS: &str = "desired_status";
/// See [`ScheduleRow::sync_status`].
const COL_SYNC_STATUS: &str = "sync_status";
/// See [`ScheduleRow::recipe`].
const COL_RECIPE: &str = "recipe";

/// A schedule row as the statements answer it, before its words are read.
#[derive(Debug, FromRow)]
struct ScheduleRow {
    /// The schedule's id, as text.
    id: String,
    /// The fleet's id, as text.
    fleet_id: String,
    /// The source word.
    source: String,
    /// The external scheduler's key.
    provider_schedule_id: String,
    /// The expression.
    cron_expression: String,
    /// The zone.
    timezone: String,
    /// The message.
    message: String,
    /// Whether it retires after one fire.
    once: bool,
    /// The intent word.
    desired_status: String,
    /// The sync word.
    sync_status: String,
    /// The fence's version.
    resource_version: i64,
    /// The fence's holder, as text.
    sync_token: Option<String>,
    /// When the fence's lease runs out.
    sync_lease_expires_at: Option<jiff_sqlx::Timestamp>,
    /// Why the last push failed.
    last_error: Option<String>,
    /// When the row was written.
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    created_at: Timestamp,
    /// When the row was last changed.
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    updated_at: Timestamp,
    /// The instant a `once` schedule is for.
    fire_at: Option<jiff_sqlx::Timestamp>,
    /// The inherited recipe, as JSON text.
    recipe: Option<String>,
}

/// What sqlx reports for a column this build cannot make sense of.
///
/// A stored `desired_status` a newer daemon wrote, or an id that is not
/// canonical. Rendered as a decode failure on that column rather than defaulted
/// past — see [`crate::model`] on what defaulting one would cost.
fn unreadable(column: &'static str) -> sqlx::Error {
    sqlx::Error::ColumnDecode {
        index: column.to_owned(),
        source: format!("{column} is not a value this build can read").into(),
    }
}

impl ScheduleRow {
    /// The schedule this row stores, its words and ids read here.
    fn parse(self) -> sqlx::Result<Schedule> {
        Ok(Schedule {
            schedule_id: Uuid7::parse(&self.id).map_err(|_shape| unreadable(COL_ID))?,
            fleet_id: Uuid7::parse(&self.fleet_id).map_err(|_shape| unreadable(COL_FLEET))?,
            source: Source::parse(&self.source).ok_or_else(|| unreadable(COL_SOURCE))?,
            provider_schedule_id: self.provider_schedule_id,
            cron: self.cron_expression,
            timezone: self.timezone,
            message: self.message,
            once: self.once,
            desired_status: DesiredStatus::parse(&self.desired_status)
                .ok_or_else(|| unreadable(COL_DESIRED_STATUS))?,
            sync_status: SyncStatus::parse(&self.sync_status)
                .ok_or_else(|| unreadable(COL_SYNC_STATUS))?,
            resource_version: self.resource_version,
            sync_token: self.sync_token,
            sync_lease_expires_at: self
                .sync_lease_expires_at
                .map(jiff_sqlx::Timestamp::to_jiff),
            last_error: self.last_error,
            created_at: self.created_at,
            updated_at: self.updated_at,
            fire_at: self.fire_at.map(jiff_sqlx::Timestamp::to_jiff),
            recipe: self
                .recipe
                .as_deref()
                .map(serde_json::from_str)
                .transpose()
                .map_err(|source| sqlx::Error::ColumnDecode {
                    index: COL_RECIPE.into(),
                    source: Box::new(source),
                })?,
        })
    }
}

impl FromRow<'_, PgRow> for Schedule {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        ScheduleRow::from_row(row)?.parse()
    }
}

#[cfg(test)]
mod tests;
