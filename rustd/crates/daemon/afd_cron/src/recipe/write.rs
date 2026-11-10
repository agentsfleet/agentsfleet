//! Effective values are projections; only the pinned base supplies Restore.

use super::{Recipe, RecipeSet, Stored, replace, sql};
use crate::store::history::{Fact, Receipt};
use crate::{Result, Schedule, Schedules, Source, SyncStatus, error};
use afd_api_wire::activity::ScheduleChange;
use sqlx::PgConnection;

const CONTEXT: &str = "write inherited schedule";

struct Projection<'a> {
    effective: &'a Recipe,
    encoded: &'a str,
}

impl Schedules {
    pub(super) async fn write_recipe(
        &self,
        connection: &mut PgConnection,
        set: &RecipeSet<'_>,
        recipe: &Recipe,
        slot: i64,
        old: Option<&Stored>,
    ) -> Result<afd_events::activity::Announcement> {
        let (inherited, changed) = replace::inherited(
            old.and_then(|row| row.recipe.as_deref()),
            recipe,
            set.setup_revision,
            slot,
        )?;
        let effective = inherited.effective();
        let encoded = serde_json::to_string(&inherited)?;
        let written = self
            .write_projection(
                connection,
                set,
                Projection {
                    effective: &effective,
                    encoded: &encoded,
                },
                slot,
                old,
            )
            .await?;
        if !changed && old.is_some_and(|row| row.desired_status == written.desired_status.as_str())
        {
            return Ok(afd_events::activity::Announcement::default());
        }
        Receipt {
            schedule: &written,
            fact: Fact::Changed(if old.is_none() {
                ScheduleChange::Created
            } else {
                ScheduleChange::Changed
            }),
        }
        .record_on(connection, set.workspace)
        .await
    }

    async fn write_projection(
        &self,
        connection: &mut PgConnection,
        set: &RecipeSet<'_>,
        projection: Projection<'_>,
        slot: i64,
        old: Option<&Stored>,
    ) -> Result<Schedule> {
        if let Some(old) = old {
            sqlx::query_as::<_, Schedule>(sql::UPDATE)
                .bind(&old.id)
                .bind(&projection.effective.cron)
                .bind(&projection.effective.timezone)
                .bind(&projection.effective.message)
                .bind(projection.effective.status.as_str())
                .bind(SyncStatus::Syncing.as_str())
                .bind(projection.encoded)
                .fetch_one(&mut *connection)
                .await
                .map_err(error::query(CONTEXT))
        } else {
            let id = self.mint(set.now)?;
            sqlx::query_as::<_, Schedule>(sql::INSERT)
                .bind(&id)
                .bind(set.fleet)
                .bind(Source::Trigger.as_str())
                .bind(format!("recipe:{slot}"))
                .bind(&projection.effective.cron)
                .bind(&projection.effective.timezone)
                .bind(&projection.effective.message)
                .bind(projection.effective.status.as_str())
                .bind(SyncStatus::Syncing.as_str())
                .bind(projection.encoded)
                .fetch_one(&mut *connection)
                .await
                .map_err(error::query(CONTEXT))
        }
    }
}
