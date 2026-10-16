//! Capacity and live fences are checked before any recipe projection is changed.

use super::{RecipeSet, Stored, sql};
use crate::{
    Refused, Result,
    error::{self, ErrorKind},
};
use sqlx::PgConnection;

const CONTEXT: &str = "lock inherited schedules";

pub(super) async fn lock(
    connection: &mut PgConnection,
    set: &RecipeSet<'_>,
) -> Result<Vec<Stored>> {
    let owned: Option<String> = sqlx::query_scalar(sql::LOCK)
        .bind(set.fleet.as_str())
        .bind(set.workspace.as_str())
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
        .bind(set.fleet.as_str())
        .fetch_all(&mut *connection)
        .await
        .map_err(error::query(CONTEXT))?;
    if rows.iter().any(|row| row.held) {
        return Err(ErrorKind::Held.into());
    }
    let independent: i64 = sqlx::query_scalar(sql::COUNT)
        .bind(set.fleet.as_str())
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
