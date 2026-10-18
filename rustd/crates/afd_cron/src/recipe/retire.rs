//! Removed recipe slots retain their row until the scheduler confirms deletion.

use super::{RecipeSet, Stored, sql};
use crate::{DesiredStatus, Result, SyncStatus, error};
use sqlx::PgConnection;

const CONTEXT: &str = "retire removed recipes";

pub(super) async fn removed(
    connection: &mut PgConnection,
    rows: &[Stored],
    set: &RecipeSet<'_>,
) -> Result<()> {
    for row in rows {
        if usize::try_from(row.slot).map_or(true, |slot| slot >= set.recipes.len()) {
            sqlx::query(sql::RETIRE)
                .bind(&row.id)
                .bind(DesiredStatus::Deleting.as_str())
                .bind(SyncStatus::Syncing.as_str())
                .execute(&mut *connection)
                .await
                .map_err(error::query(CONTEXT))?;
        }
    }
    Ok(())
}
