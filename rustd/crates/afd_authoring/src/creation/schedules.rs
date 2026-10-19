//! Schedule intent is copied on the same transaction as its setup pointer.

use super::Creations;
use crate::Result;
use afd_core::id::Uuid7;
use afd_fleet_runtime::{Definition, config::Trigger};
use jiff::Timestamp;
use sqlx::PgConnection;

impl Creations {
    pub(super) async fn replace_schedules(
        &self,
        connection: &mut PgConnection,
        workspace: &Uuid7,
        fleet: &Uuid7,
        version: i64,
        definition: &Definition,
        now: Timestamp,
    ) -> Result<()> {
        let recipes: Vec<_> = definition
            .runtime()
            .triggers()
            .iter()
            .filter_map(|trigger| match trigger {
                Trigger::Cron(cron) => Some(afd_cron::Recipe::parse(
                    &cron.schedule,
                    &cron.timezone,
                    &cron.message,
                )),
                _ => None,
            })
            .collect::<afd_cron::Result<_>>()?;
        self.schedules
            .replace_recipes_on(
                connection,
                afd_cron::RecipeSet {
                    workspace,
                    fleet,
                    setup_revision: version,
                    recipes: &recipes,
                    now,
                },
            )
            .await?;
        Ok(())
    }
}
