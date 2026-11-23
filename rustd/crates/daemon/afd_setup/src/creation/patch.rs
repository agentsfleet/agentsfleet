//! Strong tags serialize status changes, setup retries and cancellation.

use super::{
    ACTIVE, ARCHIVED, CONTEXT, Creations, INSTALLING, SavedFleet, etag, read, sql, stored::Stored,
};
use crate::{
    Access, Result, access,
    error::{
        ErrorKind,
        raise::{invalid, query},
    },
};
use af_core::id::Uuid7;
use af_observability::SetupStep;
use jiff::Timestamp;
use sqlx::{Connection as _, PgConnection};

impl Creations {
    /// Changes name/status or explicitly resumes a retained setup under its current tag.
    ///
    /// # Errors
    /// Refuses stale/missing tags, unsupported transitions and invalid slugs.
    /// An archive closes the fleet's saving draft in the same commit.
    pub async fn patch(
        &self,
        access: Access<'_>,
        fleet: &Uuid7,
        expected: Option<&str>,
        status: Option<&str>,
        name: Option<&str>,
        now: Timestamp,
    ) -> Result<SavedFleet> {
        if status.is_none() && name.is_none() {
            return Err(invalid("A name or status is required"));
        }
        if let Some(name) = name {
            af_fleet_runtime::FleetName::parse(name)?;
        }
        let mut connection = self.setup.drafts.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        access::current(&mut tx, access).await?;
        let row = read::lock(&mut tx, access.workspace, fleet).await?;
        let expected = expected.ok_or(ErrorKind::Precondition)?;
        let current = etag(fleet, row.resource_version);
        if expected != current {
            return Err(ErrorKind::Stale { etag: current }.into());
        }
        let requested = status.unwrap_or(&row.status);
        transition(&row, status)?;
        // `transition` refused an archived row, so a fleet here is installing
        // or active: a bare retry writes nothing and only re-runs setup.
        let retry = status == Some(ACTIVE);
        if !retry || name.is_some() {
            let written = if retry && row.status != ACTIVE {
                INSTALLING
            } else {
                requested
            };
            sqlx::query(sql::UPDATE)
                .bind(fleet)
                .bind(access.workspace)
                .bind(written)
                .bind(name.unwrap_or(&row.name))
                .execute(&mut *tx)
                .await
                .map_err(query(CONTEXT))?;
        }
        let cancelled = if requested == ARCHIVED {
            let cancelled = self.cancel(&mut tx, fleet).await?;
            // The draft's proposal is now the fleet's revision 1, so the draft
            // has nothing left to edit: an archive ends it.
            super::activate::close_saved_drafts(&mut tx, fleet).await?;
            cancelled
        } else {
            Vec::new()
        };
        tx.commit().await.map_err(query(CONTEXT))?;
        self.announce_cancellations(cancelled).await;
        // Archiving an installing fleet cancels its setup; a live fleet's setup
        // already finished, and `transition` refused any repeated archive above.
        if requested == ARCHIVED && row.status == INSTALLING {
            self.setup_cancelled(access, fleet);
        }
        if retry {
            self.advance(access, fleet, now).await?;
        }
        self.read(access, fleet).await
    }

    pub(super) async fn cancel(
        &self,
        connection: &mut PgConnection,
        fleet: &Uuid7,
    ) -> Result<Vec<(Uuid7, Uuid7)>> {
        self.grants.cancel_setup_on(connection, fleet).await?;
        let cancelled = sqlx::query_as(sql::CANCEL_RUNS)
            .bind(fleet)
            .bind(afd_state::sql::RUN_STATUS_ACTIVE)
            .fetch_all(connection)
            .await
            .map_err(query(CONTEXT))?;
        Ok(cancelled)
    }

    /// Reports that an installing fleet's setup was cancelled, by an archive
    /// or a delete, once its transaction committed.
    pub(super) fn setup_cancelled(&self, access: Access<'_>, fleet: &Uuid7) {
        let step = SetupStep::SetupCancelled {
            fleet_id: fleet.clone(),
        };
        self.setup.drafts.reached(access, step);
    }

    pub(super) async fn announce_cancellations(&self, cancelled: Vec<(Uuid7, Uuid7)>) {
        let bus = self.activity_streams();
        for (runner, run_id) in cancelled {
            let subject = afd_bus::Subject::RunnerCommand(runner.clone());
            let publication = afd_bus::Command::Cancel { run_id }
                .publish(&bus, &runner)
                .await;
            afd_bus::observe_notification(&subject, publication);
        }
    }
}

fn transition(row: &Stored, requested: Option<&str>) -> Result<()> {
    if row.status == ARCHIVED {
        return Err(ErrorKind::Archived.into());
    }
    if requested.is_some_and(|status| !matches!(status, ACTIVE | ARCHIVED)) {
        return Err(ErrorKind::State {
            current: "transition_refused",
        }
        .into());
    }
    Ok(())
}
