//! Deleting a fleet: the teardown an archive runs, then the row, in one
//! transaction.
//!
//! A delete takes a fleet in any status, so it can meet one that still holds
//! setup grants, live runs and a saving draft. It ends all three as an archive
//! does, closing the draft, and removes the row under one lock and one tag
//! check, so a refused or failed delete changes nothing.

use super::{CONTEXT, Creations, INSTALLING, etag, read};
use crate::{
    Access, Result, access,
    error::{ErrorKind, raise::query},
};
use af_core::id::Uuid7;
use sqlx::Connection as _;

impl Creations {
    /// Ends `fleet`'s setup grants and live runs, closes its saving draft
    /// and deletes the fleet, all under the tag `expected`.
    ///
    /// Runners hear their Cancel only after the commit, so no runner stops for
    /// a delete that rolled back; the workspace's open walls hear after it too.
    ///
    /// # Errors
    /// Refuses a caller who is not a current member, a fleet outside the
    /// workspace, a stale tag, and datastore failures.
    pub async fn delete(&self, access: Access<'_>, fleet: &Uuid7, expected: &str) -> Result<()> {
        let mut connection = self.setup.drafts.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        access::current(&mut tx, access).await?;
        let row = read::lock(&mut tx, access.workspace, fleet).await?;
        let current = etag(fleet, row.resource_version);
        if expected != current {
            return Err(ErrorKind::Stale { etag: current }.into());
        }
        let cancelled = self.cancel(&mut tx, fleet).await?;
        super::activate::close_saved_drafts(&mut tx, fleet).await?;
        afd_fleet_lifecycle::delete_locked(&mut tx, access.workspace, fleet).await?;
        tx.commit().await.map_err(query(CONTEXT))?;
        afd_turns::view::activity::fleets_changed(&self.activity_streams(), access.workspace).await;
        self.announce_cancellations(cancelled).await;
        if row.status == INSTALLING {
            self.setup_cancelled(access, fleet);
        }
        Ok(())
    }
}
