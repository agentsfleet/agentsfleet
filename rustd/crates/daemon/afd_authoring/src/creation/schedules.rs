//! Schedule intent is copied on the same transaction as its setup pointer, and
//! registered upstream once the fleet is active.

use super::{ACTIVE, Creations};
use crate::{Access, Result};
use af_core::id::Uuid7;
use af_fleet_runtime::{Definition, config::Trigger};
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
    ) -> Result<afd_events::activity::Announcement> {
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
                Trigger::Webhook(_) | Trigger::Mention(_) => None,
            })
            .collect::<afd_cron::Result<_>>()?;
        Ok(self
            .schedules
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
            .await?)
    }

    /// Registers an active fleet's trigger schedules, as Save's route does
    /// after its own activation; an installing fleet's wait for activation.
    pub(super) async fn register_triggers(
        &self,
        access: Access<'_>,
        fleet: &Uuid7,
        now: Timestamp,
    ) -> Result<()> {
        let Some(registrar) = &self.registrar else {
            return Ok(());
        };
        if self.read(access, fleet).await?.status != ACTIVE {
            return Ok(());
        }
        Ok(registrar
            .register_triggers(access.workspace, fleet, now)
            .await?)
    }
}
