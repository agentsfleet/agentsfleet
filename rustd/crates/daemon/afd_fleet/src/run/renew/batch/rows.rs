//! The frame's run rows: taken from what the session kept, or read once.

use std::collections::HashMap;

use af_core::id::Uuid7;
use af_wire::report::RenewRequest;
use jiff::Timestamp;

use crate::error::{Result, query};
use crate::run::pull::RunService;
use crate::run::renew::Renewing;
use crate::run::sql;

/// What query failures name the pre-read as.
const CONTEXT_READ: &str = "renewal batch read";

/// One run as the pre-read found it.
#[derive(Debug, sqlx::FromRow)]
pub(super) struct Found {
    pub(super) id: Uuid7,
    pub(super) status: String,
    pub(super) cancelled: bool,
    pub(super) tenant_id: Uuid7,
    pub(super) workspace_id: Uuid7,
    pub(super) fleet_id: Uuid7,
    pub(super) posture: String,
    pub(super) provider: String,
    pub(super) model: String,
    pub(super) task_id: Option<Uuid7>,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub(super) created_at: Timestamp,
    pub(super) head_revision_id: Option<Uuid7>,
    pub(super) catalogue_version: i64,
}

impl Found {
    pub(super) fn renewing(&self) -> Renewing {
        Renewing {
            cancelled: self.cancelled,
            tenant_id: self.tenant_id.clone(),
            fleet_id: self.fleet_id.clone(),
            workspace_id: self.workspace_id.clone(),
            posture: self.posture.clone(),
            provider: self.provider.clone(),
            model: self.model.clone(),
            status: self.status.clone(),
        }
    }
}

impl RunService {
    /// The frame's rows from what the session kept and the versions the
    /// replica last read, or `None` when any run or fleet lacks one: the
    /// statement guards every fact taken here, and reads status and the cancel
    /// mark itself under its locks.
    pub(super) async fn kept_rows(
        &self,
        runs: &[(Uuid7, RenewRequest)],
        kept: &HashMap<Uuid7, crate::run::Reported>,
    ) -> Option<Vec<Found>> {
        let mut rows = Vec::with_capacity(runs.len());
        for (run_id, _request) in runs {
            let facts = kept.get(run_id)?;
            let (head, catalogue_version) = self.runs.latest().known(&facts.fleet_id).await?;
            rows.push(Found {
                id: run_id.clone(),
                status: sql::RUN_STATUS_ACTIVE.to_owned(),
                cancelled: false,
                tenant_id: facts.tenant_id.clone(),
                workspace_id: facts.workspace_id.clone(),
                fleet_id: facts.fleet_id.clone(),
                posture: facts.posture.clone(),
                provider: facts.provider.clone(),
                model: facts.model.clone(),
                task_id: None,
                created_at: facts.run_created_at,
                head_revision_id: Some(head),
                catalogue_version,
            });
        }
        Some(rows)
    }

    /// The frame's rows as the pre-read answers them, which also tells the
    /// replica each fleet's versions.
    pub(super) async fn read_renewals(
        &self,
        runner_id: &Uuid7,
        runs: &[(Uuid7, RenewRequest)],
    ) -> Result<Vec<Found>> {
        let ids: Vec<&Uuid7> = runs.iter().map(|(id, _request)| id).collect();
        let found: Vec<Found> = sqlx::query_as(sql::renew::SELECT_RENEWALS)
            .bind(&ids)
            .bind(runner_id)
            .fetch_all(&mut *self.runs.pool().acquire().await?)
            .await
            .map_err(query(CONTEXT_READ))?;
        for row in &found {
            self.runs
                .latest()
                .observe(
                    &row.fleet_id,
                    row.head_revision_id.as_ref(),
                    row.catalogue_version,
                )
                .await;
        }
        Ok(found)
    }
}
