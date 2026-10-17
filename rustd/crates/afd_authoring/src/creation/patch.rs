//! Strong tags serialize status changes, setup retries and cancellation.

use super::{
    ACTIVE, CONTEXT, Creations, INSTALLING, KILLED, SavedFleet, etag, read, stored::Stored,
};
use crate::{
    Access, Result, access,
    error::{
        ErrorKind,
        raise::{invalid, query},
    },
};
use afd_core::{clock::UnixMillis, id::Uuid7};
use afd_observability::AuthoringStep;
use sqlx::{Connection as _, PgConnection};

const UPDATE: &str = "UPDATE core.fleets SET status=$3,name=$4,resource_version=resource_version+1,updated_at=$5 WHERE id=$1::uuid AND workspace_id=$2::uuid";
const RESTORE: &str = "UPDATE core.authoring_drafts SET state=$2,linked_fleet_id=NULL,closed_at=NULL,expires_at=NULL,resource_version=resource_version+1,updated_at=$3 WHERE linked_fleet_id=$1::uuid AND state=$4";
const PENDING_NAME: &str = "UPDATE core.fleets SET setup_context=jsonb_set(setup_context,'{context,instance_name}',to_jsonb($2::text)) WHERE id=$1::uuid";

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
        now: UnixMillis,
    ) -> Result<SavedFleet> {
        if status.is_none() && name.is_none() {
            return Err(invalid("A name or status is required"));
        }
        if let Some(name) = name {
            afd_fleet_runtime::FleetName::parse(name)?;
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
            sqlx::query(UPDATE)
                .bind(fleet.as_str())
                .bind(access.workspace.as_str())
                .bind(written)
                .bind(name.unwrap_or(&row.name))
                .bind(now.as_millis())
                .execute(&mut *tx)
                .await
                .map_err(query(CONTEXT))?;
        }
        if row.status == INSTALLING && name.is_some_and(|name| name != row.name) {
            sqlx::query(PENDING_NAME)
                .bind(fleet.as_str())
                .bind(name)
                .execute(&mut *tx)
                .await
                .map_err(query(CONTEXT))?;
        }
        if requested == KILLED {
            self.cancel(&mut tx, fleet, now).await?;
        }
        tx.commit().await.map_err(query(CONTEXT))?;
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
        now: UnixMillis,
    ) -> Result<()> {
        self.grants.cancel_setup_on(connection, fleet, now).await?;
        sqlx::query(RESTORE)
            .bind(fleet.as_str())
            .bind("editing")
            .bind(now.as_millis())
            .bind("saving")
            .execute(connection)
            .await
            .map_err(query(CONTEXT))?;
        Ok(())
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
