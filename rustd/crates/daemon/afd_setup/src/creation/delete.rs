//! Deleting a fleet: its live runs are marked cancelled and the row goes, in
//! one transaction under one lock and one tag check, so a refused or failed
//! delete changes nothing.
//!
//! A fleet's schedules live in an external scheduler too, which no
//! transaction reaches. [`Creations::hold_delete`] locks the row and checks
//! the tag first, and the caller removes the schedules upstream on that same
//! transaction while the lock stands, so a fleet change waits rather than
//! staling the tag between the check and the purge. A delete that still finds
//! a registered schedule commits only the removals that landed
//! ([`Creations::keep`]); the purge refuses a fleet that still holds one.

use af_core::id::Uuid7;
use afd_turns::view::activity::{self, Announcement, Notices};
use sqlx::{PgConnection, Postgres, Transaction};

use super::{CONTEXT, Creations, etag, read};
use crate::{
    Access, Result, access,
    error::{ErrorKind, raise::query},
};
use read::Stored;

/// A fleet's delete between its tag check and its purge: the fleet's row held
/// `FOR UPDATE` on one open transaction.
///
/// Dropped, it rolls back and changes nothing.
#[derive(Debug)]
#[must_use = "a held delete either purges, keeps its schedule removals, or rolls back"]
pub struct HeldDelete {
    transaction: Transaction<'static, Postgres>,
    workspace: Uuid7,
    fleet: Uuid7,
}

impl HeldDelete {
    /// The transaction the fleet's row is held on, for the writes that must
    /// land under the same lock.
    pub fn connection(&mut self) -> &mut PgConnection {
        &mut self.transaction
    }
}

impl Creations {
    /// Locks `fleet` for its delete and refuses a caller who is not a current
    /// member, a fleet outside the workspace, and a stale tag `expected`.
    ///
    /// The lock is the one a fleet change and a schedule insert wait on, so
    /// the tag checked here is the tag the purge deletes under.
    ///
    /// # Errors
    /// Those refusals, and datastore failures.
    pub async fn hold_delete(
        &self,
        access: Access<'_>,
        fleet: &Uuid7,
        expected: &str,
    ) -> Result<HeldDelete> {
        let connection = self.database.acquire().await?;
        let mut transaction = Transaction::begin(connection, None)
            .await
            .map_err(query(CONTEXT))?;
        access::current(&mut transaction, access).await?;
        let row = read::lock(&mut transaction, access.workspace, fleet).await?;
        tagged(fleet, &row, expected)?;
        Ok(HeldDelete {
            transaction,
            workspace: access.workspace.clone(),
            fleet: fleet.clone(),
        })
    }

    /// Commits what `held` wrote besides the purge, then announces those
    /// `receipts`: the schedule removals of a delete that still found one
    /// registered. The fleet stays, under the same tag.
    ///
    /// # Errors
    /// A refused commit, which announces nothing.
    pub async fn keep(&self, held: HeldDelete, receipts: Announcement) -> Result<()> {
        let notices = Notices::new(self.turns.streams(), activity::DEFAULT_ACTIVITY_NOTICES);
        activity::commit(held.transaction, receipts, Some(&notices))
            .await
            .map_err(query(CONTEXT))
    }

    /// Ends the held fleet's live runs and deletes it, then commits.
    ///
    /// Runners hear their Cancel only after the commit, so no runner stops for
    /// a delete that rolled back; the workspace's open walls hear after it too.
    ///
    /// # Errors
    /// A fleet that still holds a schedule (`UZ-SCHED-012`), and datastore
    /// failures; either rolls back the whole delete.
    pub async fn delete(&self, held: HeldDelete) -> Result<()> {
        let HeldDelete {
            mut transaction,
            workspace,
            fleet,
        } = held;
        let cancelled = super::cancel_runs(&mut transaction, &fleet).await?;
        afd_fleet_lifecycle::delete_locked(&mut transaction, &workspace, &fleet).await?;
        transaction.commit().await.map_err(query(CONTEXT))?;
        activity::fleets_changed(&self.turns.streams(), &workspace).await;
        self.announce_cancellations(cancelled).await;
        Ok(())
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
