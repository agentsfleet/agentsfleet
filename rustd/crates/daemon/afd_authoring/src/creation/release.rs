//! What a delete ends before the row goes: the same teardown an archive runs.
//!
//! Delete admits a fleet in any status (`M249_002` §6), so a fleet can be
//! deleted while it still holds setup grants, live leases and a saving draft.
//! An archive already ends exactly those, so a delete runs that teardown first
//! and then purges; it writes no status, so the purge's tag check still sees
//! the version the caller read.

use super::{CONTEXT, Creations, INSTALLING, etag, read};
use crate::{
    Access, Result, access,
    error::{ErrorKind, raise::query},
};
use af_core::id::Uuid7;
use af_observability::AuthoringStep;
use sqlx::Connection as _;

impl Creations {
    /// Cancels `fleet`'s setup grants and live leases and returns its saving
    /// draft to editing, ahead of a purge.
    ///
    /// Checked under the same tag the purge will check, so a stale delete
    /// ends no work it then fails to remove.
    ///
    /// # Errors
    /// Refuses a caller who is not a current member, a fleet outside the
    /// workspace, a stale tag, and datastore failures.
    pub async fn release(&self, access: Access<'_>, fleet: &Uuid7, expected: &str) -> Result<()> {
        let mut connection = self.setup.drafts.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        access::current(&mut tx, access).await?;
        let row = read::lock(&mut tx, access.workspace, fleet).await?;
        let current = etag(fleet, row.resource_version);
        if expected != current {
            return Err(ErrorKind::Stale { etag: current }.into());
        }
        let cancelled = self.cancel(&mut tx, fleet).await?;
        tx.commit().await.map_err(query(CONTEXT))?;
        self.announce_cancellations(cancelled).await;
        if row.status == INSTALLING {
            let step = AuthoringStep::SetupCancelled {
                fleet_id: fleet.clone(),
            };
            self.setup.drafts.reached(access, step);
        }
        Ok(())
    }
}
