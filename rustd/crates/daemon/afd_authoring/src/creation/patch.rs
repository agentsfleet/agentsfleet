//! Strong tags serialize status changes, setup retries and cancellation.

use super::{
    ACTIVE, CONTEXT, Creations, INSTALLING, KILLED, SavedFleet, etag, read, sql, stored::Stored,
};
use crate::{
    Access, Result, access,
    error::{
        ErrorKind,
        raise::{invalid, query},
    },
    types::DraftState,
};
use af_core::id::Uuid7;
use af_observability::AuthoringStep;
use jiff::Timestamp;
use sqlx::{Connection as _, PgConnection};

impl Creations {
    /// Changes name/status or explicitly resumes a retained setup under its current tag.
    ///
    /// # Errors
    /// Refuses stale/missing tags, unsupported transitions and invalid slugs.
    /// A Kill and linked-draft restoration commit together.
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
        let retry = status == Some(ACTIVE);
        if !retry || !matches!(row.status.as_str(), INSTALLING | ACTIVE) || name.is_some() {
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
        let cancelled = if requested == KILLED {
            self.cancel(&mut tx, fleet).await?
        } else {
            Vec::new()
        };
        tx.commit().await.map_err(query(CONTEXT))?;
        self.announce_cancellations(cancelled).await;
        // Killing an installing fleet cancels its setup; a live fleet's setup
        // already finished, and `transition` refused any repeated Kill above.
        if requested == KILLED && row.status == INSTALLING {
            let step = AuthoringStep::SetupCancelled {
                fleet_id: fleet.clone(),
            };
            self.setup.drafts.reached(access, step);
        }
        if retry {
            self.advance(access, fleet, now).await?;
        }
        self.read(access, fleet).await
    }

    async fn cancel(
        &self,
        connection: &mut PgConnection,
        fleet: &Uuid7,
    ) -> Result<Vec<(Uuid7, Uuid7)>> {
        self.grants.cancel_setup_on(connection, fleet).await?;
        let cancelled = sqlx::query_as(sql::CANCEL_LEASES)
            .bind(fleet)
            .bind(afd_runner::sql::LEASE_STATUS_ACTIVE)
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT))?;
        sqlx::query(sql::RESTORE)
            .bind(fleet)
            .bind(DraftState::Editing.as_str())
            .bind(DraftState::Saving.as_str())
            .execute(connection)
            .await
            .map_err(query(CONTEXT))?;
        Ok(cancelled)
    }

    async fn announce_cancellations(&self, cancelled: Vec<(Uuid7, Uuid7)>) {
        let bus = self.activity_streams();
        for (runner, lease_id) in cancelled {
            let subject = afd_bus::Subject::RunnerCommand(runner);
            let publication = bus.publish_frame(&subject, &afd_bus::Command::Cancel { lease_id }).await;
            afd_bus::observe_notification(&subject, publication);
        }
    }
}

/// A live fleet a person paused; an installing one cannot be stopped.
const STOPPED: &str = "stopped";

fn transition(row: &Stored, requested: Option<&str>) -> Result<()> {
    if row.status == KILLED {
        return Err(ErrorKind::FleetMissing.into());
    }
    if requested.is_some_and(|status| !matches!(status, ACTIVE | KILLED | STOPPED))
        || (row.status == INSTALLING && requested == Some(STOPPED))
    {
        return Err(ErrorKind::State {
            current: "transition_refused",
        }
        .into());
    }
    Ok(())
}
