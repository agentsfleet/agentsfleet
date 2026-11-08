//! Capacity and live fences are checked before any recipe projection is changed.

use super::{RecipeSet, Stored, sql};
use crate::{
    Refused, Result,
    error::{self, ErrorKind},
};
use af_core::id::Uuid7;
use sqlx::PgConnection;

const CONTEXT: &str = "lock inherited schedules";

pub(super) async fn lock(
    connection: &mut PgConnection,
    set: &RecipeSet<'_>,
) -> Result<Vec<Stored>> {
    let owned: Option<Uuid7> = sqlx::query_scalar(crate::sql::LOCK_FLEET_IN_WORKSPACE)
        .bind(set.fleet)
        .bind(set.workspace)
        .fetch_optional(&mut *connection)
        .await
        .map_err(error::query(CONTEXT))?;
    if owned.is_none() {
        return Err(ErrorKind::Refused {
            refusal: Refused::NoSuchFleet,
        }
        .into());
    }
    let rows: Vec<Stored> = sqlx::query_as(sql::ROWS)
        .bind(set.fleet)
        .fetch_all(&mut *connection)
        .await
        .map_err(error::query(CONTEXT))?;
    if rows.iter().any(|row| row.held) {
        return Err(ErrorKind::Held.into());
    }
    let independent: i64 = sqlx::query_scalar(crate::sql::COUNT_INDEPENDENT)
        .bind(set.fleet)
        .fetch_one(connection)
        .await
        .map_err(error::query(CONTEXT))?;
    let count = usize::try_from(independent)
        .unwrap_or(usize::MAX)
        .saturating_add(set.recipes.len());
    if count > crate::MAX_SCHEDULES_PER_FLEET {
        return Err(ErrorKind::Refused {
            refusal: Refused::TooMany,
        }
        .into());
    }
    Ok(rows)
}
