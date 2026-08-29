//! One row, as the value type.
//!
//! Split from [`super`] because it is where every statement's correctness lands.
//! Columns are read BY NAME: a positional decoder makes the order in the
//! statement load-bearing, and two `TEXT` columns transposed there would read
//! silently swapped — `source_key` into `cron_expression` is a schedule that
//! registers upstream under a cron expression as its key. A name cannot be
//! transposed, and a name that is wrong fails loudly on the first row.
//!
//! `FromRow` rather than a free function, matching `afd_billing::Wallet`: it
//! folds every column read into the one failure sqlx already reports for the
//! query, so a caller has a single error path.

use afd_core::id::Uuid7;
use sqlx::postgres::PgRow;
use sqlx::{FromRow, Row as _};

use crate::model::{DesiredStatus, Schedule, Source, SyncStatus};

/// The column each field is read from, named once.
const COL_ID: &str = "id";
/// See [`COL_ID`].
const COL_FLEET: &str = "fleet_id";
/// See [`COL_ID`].
const COL_SOURCE: &str = "source";
/// See [`COL_ID`].
const COL_SOURCE_KEY: &str = "source_key";
/// See [`COL_ID`].
const COL_CRON: &str = "cron_expression";
/// See [`COL_ID`].
const COL_TIMEZONE: &str = "timezone";
/// See [`COL_ID`].
const COL_MESSAGE: &str = "message";
/// See [`COL_ID`].
const COL_DESIRED_STATUS: &str = "desired_status";
/// See [`COL_ID`].
const COL_SYNC_STATUS: &str = "sync_status";
/// See [`COL_ID`].
const COL_GENERATION: &str = "generation";
/// See [`COL_ID`].
const COL_SYNC_TOKEN: &str = "sync_token";
/// See [`COL_ID`].
const COL_SYNC_LEASE: &str = "sync_lease_until";
/// See [`COL_ID`].
const COL_LAST_ERROR: &str = "last_error";
/// See [`COL_ID`].
const COL_CREATED_AT: &str = "created_at";
/// See [`COL_ID`].
const COL_UPDATED_AT: &str = "updated_at";

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

impl FromRow<'_, PgRow> for Schedule {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        let schedule_id: String = row.try_get(COL_ID)?;
        let fleet_id: String = row.try_get(COL_FLEET)?;
        let source: String = row.try_get(COL_SOURCE)?;
        let desired_status: String = row.try_get(COL_DESIRED_STATUS)?;
        let sync_status: String = row.try_get(COL_SYNC_STATUS)?;

        Ok(Self {
            schedule_id: Uuid7::parse(&schedule_id).map_err(|_shape| unreadable(COL_ID))?,
            fleet_id: Uuid7::parse(&fleet_id).map_err(|_shape| unreadable(COL_FLEET))?,
            source: Source::parse(&source).ok_or_else(|| unreadable(COL_SOURCE))?,
            source_key: row.try_get(COL_SOURCE_KEY)?,
            cron: row.try_get(COL_CRON)?,
            timezone: row.try_get(COL_TIMEZONE)?,
            message: row.try_get(COL_MESSAGE)?,
            desired_status: DesiredStatus::parse(&desired_status)
                .ok_or_else(|| unreadable(COL_DESIRED_STATUS))?,
            sync_status: SyncStatus::parse(&sync_status)
                .ok_or_else(|| unreadable(COL_SYNC_STATUS))?,
            generation: row.try_get(COL_GENERATION)?,
            sync_token: row.try_get(COL_SYNC_TOKEN)?,
            sync_lease_until: row.try_get(COL_SYNC_LEASE)?,
            last_error: row.try_get(COL_LAST_ERROR)?,
            created_at: row.try_get(COL_CREATED_AT)?,
            updated_at: row.try_get(COL_UPDATED_AT)?,
        })
    }
}
