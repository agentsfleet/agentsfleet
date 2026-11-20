//! Closed drafts past retention are purged, and replay receipts past their
//! retry window expire.

mod privacy;
mod sql;

use crate::{Drafts, Result};

/// Bounded counts from one daemon maintenance pass.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Maintenance {
    /// Expired private drafts removed, including transcript and candidates.
    pub purged_drafts: u64,
    /// Replay receipts deleted after their retry window closed.
    pub expired_receipts: u64,
}

impl Drafts {
    /// The privacy purges and the replay receipts' expiry, on setup's own
    /// clock: what the supervised daemon worker runs.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn housekeep(&self) -> Result<Maintenance> {
        Ok(Maintenance {
            purged_drafts: privacy::drafts(self).await?,
            expired_receipts: crate::replay::sweep(&mut *self.database.acquire().await?).await?,
        })
    }
}
