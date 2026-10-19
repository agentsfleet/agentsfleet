//! Effective values are projections; only the pinned base supplies Restore.

use super::{Recipe, RecipeSet, Stored, replace, sql};
use crate::{Result, Schedules, Source, SyncStatus, error};
use sqlx::PgConnection;

const CONTEXT: &str = "write inherited schedule";

impl Schedules {
    pub(super) async fn write_recipe(
        &self,
        connection: &mut PgConnection,
        set: &RecipeSet<'_>,
        recipe: &Recipe,
        slot: i64,
        old: Option<&Stored>,
    ) -> Result<()> {
        let inherited = replace::inherited(
            old.and_then(|row| row.recipe.as_deref()),
            recipe,
            set.setup_revision,
            slot,
        )?;
        let effective = inherited.effective();
        let encoded = serde_json::to_string(&inherited)?;
        if let Some(old) = old {
            sqlx::query(sql::UPDATE)
                .bind(&old.id)
                .bind(&effective.cron)
                .bind(&effective.timezone)
                .bind(&effective.message)
                .bind(effective.status.as_str())
                .bind(SyncStatus::Syncing.as_str())
                .bind(encoded)
                .execute(connection)
                .await
                .map_err(error::query(CONTEXT))?;
        } else {
            let id = self.mint(set.now)?;
            sqlx::query(sql::INSERT)
                .bind(id.as_str())
                .bind(set.fleet.as_str())
                .bind(Source::Trigger.as_str())
                .bind(format!("recipe:{slot}"))
                .bind(&effective.cron)
                .bind(&effective.timezone)
                .bind(&effective.message)
                .bind(effective.status.as_str())
                .bind(SyncStatus::Syncing.as_str())
                .bind(encoded)
                .execute(connection)
                .await
                .map_err(error::query(CONTEXT))?;
        }
        Ok(())
    }
}
