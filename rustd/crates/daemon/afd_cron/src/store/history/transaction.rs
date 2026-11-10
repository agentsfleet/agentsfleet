//! Parent scope precedes child writes and receipt foreign-key checks.

use af_core::id::Uuid7;
use afd_events::activity::Announcement;
use sqlx::{Postgres, Transaction};

use super::super::{CONTEXT_WRITE, Schedules};
use crate::{Result, error};

const LOCK_SCOPE: &str = include_str!("scope.sql");

/// An owned connection releases before notifying the existing live tail.
#[derive(Debug)]
pub(crate) struct TransactionScope {
    pub(crate) transaction: Transaction<'static, Postgres>,
    pub(crate) workspace: Option<Uuid7>,
}

impl Schedules {
    pub(crate) async fn history_transaction(&self, fleet: &Uuid7) -> Result<TransactionScope> {
        let mut transaction = Transaction::begin(self.database.acquire().await?, None)
            .await
            .map_err(error::query(CONTEXT_WRITE))?;
        let workspace: Option<Uuid7> = sqlx::query_scalar(LOCK_SCOPE)
            .bind(fleet)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(error::query(CONTEXT_WRITE))?;
        // Keep this transaction even when the parent is absent: a runner's
        // guard must still answer before a missing schedule is reported.
        Ok(TransactionScope {
            transaction,
            workspace,
        })
    }

    pub(crate) async fn commit_activity(
        &self,
        transaction: Transaction<'static, Postgres>,
        announcement: Announcement,
    ) -> Result<()> {
        afd_events::activity::commit(transaction, announcement, self.streams.as_ref())
            .await
            .map_err(error::query(CONTEXT_WRITE))
    }
}
