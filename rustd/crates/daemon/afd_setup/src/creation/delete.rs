//! Deleting a fleet: its live runs are marked cancelled and the row goes, in
//! one transaction under one lock and one tag check, so a refused or failed
//! delete changes nothing.
//!
//! A fleet's schedules live in an external scheduler too, which this
//! transaction cannot reach. The caller removes them there first, after
//! [`Creations::confirm_delete`] has refused what the delete would refuse,
//! and the purge refuses a fleet that still holds one.

use af_core::id::Uuid7;
use sqlx::Connection as _;

use super::{CONTEXT, Creations, etag, read};
use crate::{
    Access, Result, access,
    error::{ErrorKind, raise::query},
};
use read::Stored;

impl Creations {
    /// Ends `fleet`'s live runs and deletes it, under the tag `expected`.
    ///
    /// Runners hear their Cancel only after the commit, so no runner stops for
    /// a delete that rolled back; the workspace's open walls hear after it too.
    ///
    /// # Errors
    /// A caller who is not a current member, a fleet outside the workspace, a
    /// stale tag, and datastore failures.
    pub async fn delete(&self, access: Access<'_>, fleet: &Uuid7, expected: &str) -> Result<()> {
        let mut connection = self.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        access::current(&mut tx, access).await?;
        let row = read::lock(&mut tx, access.workspace, fleet).await?;
        tagged(fleet, &row, expected)?;
        let cancelled = super::cancel_runs(&mut tx, fleet).await?;
        afd_fleet_lifecycle::delete_locked(&mut tx, access.workspace, fleet).await?;
        tx.commit().await.map_err(query(CONTEXT))?;
        afd_turns::view::activity::fleets_changed(&self.turns.streams(), access.workspace).await;
        self.announce_cancellations(cancelled).await;
        Ok(())
    }

    /// Refuses, holding nothing, what [`Self::delete`] would refuse: a caller
    /// who is not a current member, a fleet outside the workspace, a stale
    /// tag.
    ///
    /// Run before the fleet's schedules are removed upstream, so a delete
    /// that was never going to happen removes none of them. The delete
    /// checks all of it again under its lock.
    ///
    /// # Errors
    /// As [`Self::delete`].
    pub async fn confirm_delete(
        &self,
        access: Access<'_>,
        fleet: &Uuid7,
        expected: &str,
    ) -> Result<()> {
        let mut connection = self.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        access::unlocked(&mut tx, access).await?;
        let row = read::stored(&mut tx, access.workspace, fleet).await?;
        tx.commit().await.map_err(query(CONTEXT))?;
        tagged(fleet, &row, expected)
    }
}

/// Refuses a tag that is not `row`'s current one.
fn tagged(fleet: &Uuid7, row: &Stored, expected: &str) -> Result<()> {
    let current = etag(fleet, row.resource_version);
    if expected != current {
        return Err(ErrorKind::Stale { etag: current }.into());
    }
    Ok(())
}
