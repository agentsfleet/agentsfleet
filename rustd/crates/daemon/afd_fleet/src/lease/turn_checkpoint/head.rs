//! Absence permits a clean start; present but incompatible state is an error.

use af_wire::lease::TurnResume;

use super::{TurnLease, allocation, guard::TurnLeaseGuard, row::Plan, transaction};
use crate::{Result, lease::Leases};

impl Leases {
    /// Resolve the event's last committed generation under the newly issued lease.
    ///
    /// # Errors
    /// Refuses stale authority or incompatible present state; datastore failures
    /// never become a missing checkpoint or permission to start a fresh model run.
    pub async fn turn_resume(&self, lease: TurnLease<'_>) -> Result<Option<TurnResume>> {
        let mut connection = transaction::acquire(self.pool()).await?;
        let mut owner = TurnLeaseGuard::lock(self, &mut connection, lease).await?;
        let descriptor = match allocation::head(&mut owner).await? {
            None => None,
            Some(id) => Some(Plan::load(&mut owner, &id).await?.descriptor(&owner)?),
        };
        owner.commit().await?;
        transaction::recycle(connection);
        Ok(descriptor)
    }
}
