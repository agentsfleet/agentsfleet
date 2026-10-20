//! Interrupted work stays visible without inventing provider usage or freeing debt.

mod attempts;
mod privacy;
mod sql;

use crate::{Drafts, Result};

/// Bounded counts from one daemon maintenance pass.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Maintenance {
    /// Expired live attempts moved to accounting recovery.
    pub interrupted: u64,
    /// Expired private drafts removed, including transcript and candidates.
    pub purged_drafts: u64,
    /// Provider receipts whose private result was removed.
    pub expired_results: u64,
    /// Replay receipts deleted after their retry window closed.
    pub expired_receipts: u64,
}

impl Drafts {
    /// The compiled lapse statement, for real database query-plan measurements.
    #[cfg(feature = "test-util")]
    pub const EXPIRED_ATTEMPTS_QUERY: &'static str = sql::CANDIDATES;

    /// Runs the bounded cleanup used by the supervised daemon worker.
    ///
    /// # Errors
    /// Propagates store failures; no guessed usage or automatic model resend occurs.
    pub async fn maintain(&self) -> Result<Maintenance> {
        Ok(Maintenance {
            interrupted: attempts::expire(self).await?,
            purged_drafts: privacy::drafts(self).await?,
            expired_results: privacy::results(self).await?,
            expired_receipts: crate::replay::sweep(&mut *self.database.acquire().await?).await?,
        })
    }
}
