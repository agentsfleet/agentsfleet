//! A fleet's spend against its budget, read from `billing.usage_lines`.
//!
//! Separate from the [`Biller`](crate::biller::Biller) on purpose: a budget is
//! a cap on what a fleet may spend, read from our own usage log, so it holds
//! whichever biller moves the money.

use afd_core::id::Uuid7;
use sqlx::PgExecutor;

use crate::error::query;
use crate::{Nanos, Result, Spend};

const SELECT_FLEET_SPEND: &str = include_str!("select_fleet_spend.sql");

/// What the statement was doing, for [`query`].
const CONTEXT_SPEND: &str = "read a fleet's spend";

/// The two sums, by column name.
#[derive(Debug, sqlx::FromRow)]
struct Spent {
    day_nanos: i64,
    month_nanos: i64,
}

/// What `fleet_id` has spent inside the rolling day and the UTC month.
///
/// Both windows open from `executor`'s `now()`, so a caller inside a
/// transaction measures against the instant every other statement in it sees.
///
/// # Errors
/// The statement would not run. The caller's outage policy decides what that
/// means for the work.
pub async fn fleet_spend<'e>(
    executor: impl PgExecutor<'e>,
    workspace_id: &Uuid7,
    fleet_id: &Uuid7,
) -> Result<Spend> {
    let spent: Spent = sqlx::query_as(SELECT_FLEET_SPEND)
        .bind(workspace_id.as_str())
        .bind(fleet_id.as_str())
        .bind(afd_core::timing::DAY)
        .fetch_one(executor)
        .await
        .map_err(query(CONTEXT_SPEND))?;
    Ok(Spend {
        day: Nanos::from_i64(spent.day_nanos),
        month: Nanos::from_i64(spent.month_nanos),
    })
}
