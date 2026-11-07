//! Removed recipe slots retain their row until the scheduler confirms deletion.

use super::{RecipeSet, Stored, sql};
use crate::store::history::{Fact, Receipt};
use crate::{DesiredStatus, Result, Schedule, SyncStatus, error};
use sqlx::PgConnection;

const CONTEXT: &str = "retire removed recipes";

pub(super) async fn removed(
    connection: &mut PgConnection,
    rows: &[Stored],
    set: &RecipeSet<'_>,
) -> Result<afd_events::activity::Announcement> {
    let mut announcement = afd_events::activity::Announcement::default();
    for row in rows {
        if usize::try_from(row.slot).map_or(true, |slot| slot >= set.recipes.len()) {
            let retired = sqlx::query_as::<_, Schedule>(sql::RETIRE)
                .bind(&row.id)
                .bind(DesiredStatus::Deleting.as_str())
                .bind(SyncStatus::Syncing.as_str())
                .fetch_one(&mut *connection)
                .await
                .map_err(error::query(CONTEXT))?;
            if row.desired_status != DesiredStatus::Deleting.as_str() {
                announcement.append(
                    Receipt {
                        schedule: &retired,
                        operation: "setup",
                        fact: Fact::mutation(Some(DesiredStatus::Deleting)),
                    }
                    .record_on(connection, set.workspace)
                    .await?,
                );
            }
        }
    }
    Ok(announcement)
}
