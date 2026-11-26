//! A fenced synchronization result and its history commit together.

use af_core::id::Uuid7;
use afd_turns::view::activity::Announcement;
use sqlx::PgConnection;

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
    const fn update(self) -> Update<'a> {
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
        let Some(workspace) = scope.workspace.clone() else {
            return Ok(None);
        };
        let Some((row, announcement)) =
            complete_on(&mut scope.transaction, &workspace, held, token, outcome).await?
        else {
            return Ok(None);
        };
        self.commit_activity(scope.transaction, announcement)
            .await?;
        Ok(Some(row))
    }

    /// [`Self::finalize_failed`] on the caller's connection, with the receipt
    /// that records it; `None` when the fence was lost.
    ///
    /// The caller holds the fleet's row and commits, then announces.
    pub(crate) async fn finalize_failed_on(
        &self,
        connection: &mut PgConnection,
        workspace: &Uuid7,
        held: &Schedule,
        token: &Uuid7,
        reason: &str,
    ) -> Result<Option<Announcement>> {
        let completed =
            complete_on(connection, workspace, held, token, Outcome::Failed(reason)).await?;
        Ok(completed.map(|(_, announcement)| announcement))
    }

    /// Deletes a confirmed removal with its held display snapshot in history.
    /// A missed unregistered one-off records expiry, without claiming a fire.
    ///
    /// # Errors
    /// As [`Self::finalize_synced`].
    pub async fn delete_claimed(&self, held: &Schedule, token: &Uuid7) -> Result<bool> {
        let mut scope = self.history_transaction(&held.fleet_id).await?;
        let Some(workspace) = scope.workspace.clone() else {
            return Ok(false);
        };
        let Some(announcement) = self
            .delete_claimed_on(&mut scope.transaction, &workspace, held, token)
            .await?
        else {
            return Ok(false);
        };
        self.commit_activity(scope.transaction, announcement)
            .await?;
        Ok(true)
    }

    /// [`Self::delete_claimed`] on the caller's connection, with the receipt
    /// that records it; `None` when the fence was lost.
    ///
    /// The caller holds the fleet's row and commits, then announces.
    pub(crate) async fn delete_claimed_on(
        &self,
        connection: &mut PgConnection,
        workspace: &Uuid7,
        held: &Schedule,
        token: &Uuid7,
    ) -> Result<Option<Announcement>> {
        let deleted = sqlx::query(sql::DELETE_CLAIMED)
            .bind(&held.schedule_id)
            .bind(held.resource_version)
            .bind(token)
            .fetch_optional(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_WRITE))?;
        if deleted.is_none() {
            return Ok(None);
        }
        let announcement = Receipt {
            schedule: held,
            fact: Fact::Removed,
        }
        .record_on(connection, workspace)
        .await?;
        Ok(Some(announcement))
    }
}

/// Releases the fence with `outcome` on `connection`, and records the receipt;
/// `None` when the version or token no longer holds.
async fn complete_on(
    connection: &mut PgConnection,
    workspace: &Uuid7,
    held: &Schedule,
    token: &Uuid7,
    outcome: Outcome<'_>,
) -> Result<Option<(Schedule, Announcement)>> {
    let update = outcome.update();
    let row = sqlx::query_as::<_, Schedule>(update.statement)
        .bind(&held.schedule_id)
        .bind(held.resource_version)
        .bind(token)
        .bind(update.status.as_str())
        .bind(update.value)
        .fetch_optional(&mut *connection)
        .await
        .map_err(error::query(CONTEXT_WRITE))?;
    let Some(row) = row else { return Ok(None) };
    let announcement = Receipt {
        schedule: &row,
        fact: update.fact,
    }
    .record_on(connection, workspace)
    .await?;
    Ok(Some((row, announcement)))
}
