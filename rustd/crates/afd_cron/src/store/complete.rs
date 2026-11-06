//! A fenced synchronization result and its history commit together.

use afd_core::id::Uuid7;

use super::history::{Fact, Receipt};
use super::{CONTEXT_WRITE, Schedules};
use crate::{Result, Schedule, SyncStatus, error, sql};

#[derive(Debug, Clone, Copy)]
enum Outcome<'a> {
    Synced(Option<&'a str>),
    Failed(&'a str),
}

struct Update<'a> {
    statement: &'static str,
    status: SyncStatus,
    value: Option<&'a str>,
    fact: Fact<'a>,
}

impl<'a> Outcome<'a> {
    fn update(self) -> Update<'a> {
        match self {
            Self::Synced(registered) => Update {
                statement: sql::FINALIZE_SUCCESS,
                status: SyncStatus::Synced,
                value: registered,
                fact: Fact::Synced,
            },
            Self::Failed(reason) => Update {
                statement: sql::FINALIZE_FAILURE,
                status: SyncStatus::Failed,
                value: Some(reason),
                fact: Fact::Failed(reason),
            },
        }
    }
}

impl Schedules {
    /// Confirms synchronization only while the original version/token still holds.
    /// `registered` replaces the upstream key when supplied; absence retains it.
    ///
    /// # Errors
    /// Reports unavailable storage, unreadable rows or an uncommittable receipt.
    pub async fn finalize_synced(
        &self,
        held: &Schedule,
        token: &Uuid7,
        registered: Option<&str>,
    ) -> Result<Option<Schedule>> {
        self.complete(held, token, Outcome::Synced(registered))
            .await
    }

    /// Commits a failed synchronization and its safe stored refusal sentence.
    /// Superseded claims write neither state nor history.
    ///
    /// # Errors
    /// As [`Self::finalize_synced`].
    pub async fn finalize_failed(
        &self,
        held: &Schedule,
        token: &Uuid7,
        reason: &str,
    ) -> Result<Option<Schedule>> {
        self.complete(held, token, Outcome::Failed(reason)).await
    }

    async fn complete(
        &self,
        held: &Schedule,
        token: &Uuid7,
        outcome: Outcome<'_>,
    ) -> Result<Option<Schedule>> {
        let mut scope = self.history_transaction(&held.fleet_id).await?;
        let Some(workspace) = scope.workspace.as_ref() else {
            return Ok(None);
        };
        let update = outcome.update();
        let row = sqlx::query_as::<_, Schedule>(update.statement)
            .bind(held.schedule_id.as_str())
            .bind(held.resource_version)
            .bind(token.as_str())
            .bind(update.status.as_str())
            .bind(update.value)
            .fetch_optional(&mut *scope.transaction)
            .await
            .map_err(error::query(CONTEXT_WRITE))?;
        let Some(row) = row else { return Ok(None) };
        let announcement = Receipt {
            schedule: &row,
            operation: token.as_str(),
            fact: update.fact,
        }
        .record_on(&mut scope.transaction, workspace)
        .await?;
        self.commit_activity(scope.transaction, announcement)
            .await?;
        Ok(Some(row))
    }

    /// Deletes a confirmed removal with its held display snapshot in history.
    /// A missed unregistered one-off records expiry, without claiming a fire.
    ///
    /// # Errors
    /// As [`Self::finalize_synced`].
    pub async fn delete_claimed(&self, held: &Schedule, token: &Uuid7) -> Result<bool> {
        let mut scope = self.history_transaction(&held.fleet_id).await?;
        let Some(workspace) = scope.workspace.as_ref() else {
            return Ok(false);
        };
        let deleted = sqlx::query(sql::DELETE_CLAIMED)
            .bind(held.schedule_id.as_str())
            .bind(held.resource_version)
            .bind(token.as_str())
            .fetch_optional(&mut *scope.transaction)
            .await
            .map_err(error::query(CONTEXT_WRITE))?;
        if deleted.is_none() {
            return Ok(false);
        }
        let announcement = Receipt {
            schedule: held,
            operation: token.as_str(),
            fact: Fact::Removed,
        }
        .record_on(&mut scope.transaction, workspace)
        .await?;
        self.commit_activity(scope.transaction, announcement)
            .await?;
        Ok(true)
    }
}
