//! Daemon-written setup, such as a chat channel's resident, stored the native way.
//!
//! A resident's configuration is code (RULE PRI): no person writes or reviews
//! it, so it skips the private draft and the review digest a Save presents. It
//! still lands as an immutable definition revision, so the lease, the fleet
//! read, Kill and purge treat it exactly like a saved fleet.

use super::{
    ACTIVE, CONTEXT, Creations, INITIAL_VERSION, INSTALLING, activate::EXTERNAL_LIMIT,
    reserve::Instance, source::Captured, sql,
};
use crate::{
    Result,
    error::{ErrorKind, raise::query},
    review,
    types::{Candidate, Start},
};
use afd_core::{error_code, id::Uuid7};
use afd_fleet_runtime::{
    Definition,
    definition::{POLICY_SEMANTICS, canonical_digest},
};
use jiff::Timestamp;
use sqlx::Connection as _;

/// One daemon-written fleet, before it exists.
#[derive(Debug, Clone, Copy)]
pub struct SystemSetup<'a> {
    /// The workspace the fleet is installed into.
    pub workspace: &'a Uuid7,
    /// The complete setup; its configuration name is the instance slug.
    pub definition: &'a Definition,
}

impl Creations {
    /// Installs daemon-written setup as an active fleet, its stream included.
    ///
    /// # Errors
    /// `AGENTSFLEET_NAME_EXISTS` when the workspace already holds the name, so
    /// the caller can decide whether that fleet is the one it meant. A stream
    /// that cannot be created removes the row again, so a retry starts clean.
    /// Datastore failures are preserved.
    pub async fn materialise(&self, setup: SystemSetup<'_>, now: Timestamp) -> Result<Uuid7> {
        let id = self.reserve_system(setup, now).await?;
        if let Err(unfinished) = self.open_system(setup.workspace, &id).await {
            self.remove_system(setup.workspace, &id).await;
            return Err(unfinished);
        }
        Ok(id)
    }

    async fn reserve_system(&self, setup: SystemSetup<'_>, now: Timestamp) -> Result<Uuid7> {
        setup.definition.validate_for_save()?;
        let mut connection = self.setup.drafts.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let tenant: Option<String> = sqlx::query_scalar(sql::TENANT)
            .bind(setup.workspace.as_str())
            .fetch_optional(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        let tenant = Uuid7::parse(&tenant.ok_or(ErrorKind::Hidden)?)?;
        let name = setup.definition.runtime().name().as_str();
        let captured = Captured {
            candidate: Candidate {
                id: self.setup.drafts.mint(now)?,
                revision: INITIAL_VERSION.to_string(),
                definition: setup.definition.clone(),
                canonical_digest: canonical_digest(setup.definition)?,
                policy_semantics: POLICY_SEMANTICS.to_owned(),
            },
            context: review::Context::system(setup.workspace.clone(), tenant, name),
            draft: None,
        };
        let checked = review::build(
            &self.setup,
            &mut tx,
            &captured.context,
            &captured.candidate,
            now,
        )
        .await?;
        let id = self.setup.drafts.mint(now)?;
        let instance = Instance {
            name,
            start: &Start::None,
        };
        self.insert(&mut tx, &id, &captured, instance, &checked, now)
            .await?;
        tx.commit().await.map_err(query(CONTEXT))?;
        Ok(id)
    }

    /// The stream and the flip share one failure policy: both, or neither.
    async fn open_system(&self, workspace: &Uuid7, id: &Uuid7) -> Result<()> {
        tokio::time::timeout(EXTERNAL_LIMIT, self.streams.ensure_group(id.as_str()))
            .await
            .map_err(|_elapsed| ErrorKind::RolledBack { source: None })?
            .map_err(|source| ErrorKind::RolledBack {
                source: Some(source),
            })?;
        let mut connection = self.setup.drafts.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        sqlx::query(sql::STREAM_READY)
            .bind(id.as_str())
            .bind(workspace.as_str())
            .bind(INSTALLING)
            .execute(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        sqlx::query(sql::ACTIVATE)
            .bind(id.as_str())
            .bind(workspace.as_str())
            .bind(ACTIVE)
            .bind(INSTALLING)
            .bind(INITIAL_VERSION)
            .execute(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        tx.commit().await.map_err(query(CONTEXT))
    }

    /// Best effort: the caller already holds the failure it reports.
    async fn remove_system(&self, workspace: &Uuid7, id: &Uuid7) {
        let removed = match self.setup.drafts.database.acquire().await {
            Ok(mut connection) => sqlx::query(sql::REMOVE)
                .bind(id.as_str())
                .bind(workspace.as_str())
                .bind(INSTALLING)
                .execute(connection.as_mut())
                .await
                .map(drop)
                .map_err(query(CONTEXT)),
            Err(unavailable) => Err(unavailable.into()),
        };
        if let Err(failed) = removed {
            tracing::warn!(
                event = "system_setup_rollback_failed",
                fleet_id = %id,
                error_code = error_code::INTERNAL_OPERATION_FAILED.as_str(),
                cause_error_code = failed.code().as_str(),
            );
        }
    }
}
