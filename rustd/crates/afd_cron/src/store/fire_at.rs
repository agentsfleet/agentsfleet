//! The instant a `once` schedule is for, computed from the row's own stamp.
//!
//! The expression is parsed here, in Rust, so Rust needs an instant to count
//! from. It takes the one Postgres just stamped the row with — `created_at` on
//! a create, `updated_at` on a claim or an edit — so the row carries a single
//! clock and `fire_at` is never earlier than the write that set it.

use afd_core::id::Uuid7;
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;
use sqlx::PgConnection;

use crate::error::{self, Result};
use crate::model::Schedule;
use crate::sql;

/// The context a failed fire-instant write reports under.
const CONTEXT: &str = "set a one-off's fire instant";

/// Sets `held`'s `fire_at` from its expression and zone, counted from `from`.
///
/// Conditioned on `token`, so only the write that holds the fence sets it. An
/// expression or zone that does not read stores NULL, the reading a recurring
/// schedule has.
///
/// # Errors
/// Reports a statement that failed or a row this build cannot read.
pub(super) async fn stamp(
    connection: &mut PgConnection,
    held: &Schedule,
    token: &Uuid7,
    from: Timestamp,
) -> Result<Schedule> {
    let fire_at = crate::next::next_fire(&held.cron, &held.timezone, from);
    sqlx::query_as::<_, Schedule>(sql::SET_FIRE_AT)
        .bind(held.schedule_id.as_str())
        .bind(token.as_str())
        .bind(fire_at.map(Timestamp::to_sqlx))
        .fetch_one(connection)
        .await
        .map_err(error::query(CONTEXT))
}
