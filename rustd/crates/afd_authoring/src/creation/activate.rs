//! External setup work is bounded; only a fresh locked recheck can activate.

use super::{ACTIVE, CONTEXT, Creations, INSTALLING, progress, read, sql};
use crate::{
    Access, Result, access,
    error::raise::query,
    types::{CheckKind, CheckState, DraftState},
};
use afd_approval::{Origin, Wanted};
use afd_core::id::Uuid7;
use afd_observability::AuthoringStep;
use jiff::Timestamp;
use sqlx::Connection as _;
use std::time::Duration;

pub(super) const EXTERNAL_LIMIT: Duration = Duration::from_secs(5);
const SETUP_BUDGET: Duration = Duration::from_secs(15);

impl Creations {
    pub(super) async fn advance(
        &self,
        access: Access<'_>,
        fleet: &Uuid7,
        now: Timestamp,
    ) -> Result<()> {
        match tokio::time::timeout(SETUP_BUDGET, self.advance_within(access, fleet, now)).await {
            Ok(result) => result,
            Err(_elapsed) => {
                tracing::warn!(event = "fleet_setup_budget_elapsed", fleet_id = %fleet,
                    error_code = afd_core::error_code::INTERNAL_OPERATION_FAILED.as_str());
                Ok(())
            }
        }
    }

    async fn advance_within(
        &self,
        access: Access<'_>,
        fleet: &Uuid7,
        now: Timestamp,
    ) -> Result<()> {
        let (row, checked) = self.snapshot(access, fleet).await?;
        if row.status == ACTIVE {
            self.recover_initial(access, fleet).await?;
            return self.confirm_initial(access, fleet).await;
        }
        if row.status != INSTALLING || row.setup_context.review_digest != checked.review_digest {
            return Ok(());
        }
        for check in &checked.readiness {
            if check.kind == CheckKind::Grant && check.state == CheckState::Missing {
                self.grants
                    .request_for_setup(
                        access.workspace,
                        fleet,
                        Wanted {
                            service: &check.subject,
                            credential: &check.subject,
                            origin: Origin::Install,
                        },
                        now,
                    )
                    .await?;
            }
        }
        let (row, checked) = self.snapshot(access, fleet).await?;
        if row.status != INSTALLING || !progress::ready(&row, &checked) {
            return Ok(());
        }
        if row.setup_stream_ready != Some(true) {
            if !matches!(
                tokio::time::timeout(EXTERNAL_LIMIT, self.streams.ensure_group(fleet.as_str()))
                    .await,
                Ok(Ok(()))
            ) {
                tracing::warn!(event = "fleet_setup_stream_pending", fleet_id = %fleet,
                    error_code = afd_core::error_code::INTERNAL_OPERATION_FAILED.as_str());
                return Ok(());
            }
            let mut connection = self.setup.drafts.database.acquire().await?;
            sqlx::query(sql::STREAM_READY)
                .bind(fleet.as_str())
                .bind(access.workspace.as_str())
                .bind(INSTALLING)
                .execute(&mut *connection)
                .await
                .map_err(query(CONTEXT))?;
        }
        self.activate(access, fleet).await?;
        self.confirm_initial(access, fleet).await
    }

    async fn activate(&self, access: Access<'_>, fleet: &Uuid7) -> Result<()> {
        let (snapshot, _) = self.snapshot(access, fleet).await?;
        let key = super::start::key(fleet);
        let payload = super::start::payload(&snapshot.initial_start.choice)?;
        let prepared = self
            .prepare_initial(&snapshot, &key, payload.as_deref())
            .await?;
        let mut connection = self.setup.drafts.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let person = access::current(&mut tx, access).await?;
        sqlx::query_scalar::<_, i64>(sql::WALLET)
            .bind(person.owner.as_str())
            .fetch_optional(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        let row = read::lock(&mut tx, access.workspace, fleet).await?;
        if row.status != INSTALLING || row.setup_version != snapshot.setup_version {
            return Ok(());
        }
        let checked = self.review(&mut tx, &row).await?;
        if !progress::ready(&row, &checked) || row.setup_stream_ready != Some(true) {
            return Ok(());
        }
        let initial = self
            .stage_initial(&mut tx, row.initial_start, prepared)
            .await?;
        let changed = sqlx::query(sql::ACTIVATE)
            .bind(fleet.as_str())
            .bind(access.workspace.as_str())
            .bind(ACTIVE)
            .bind(INSTALLING)
            .bind(row.setup_version)
            .execute(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        if changed.rows_affected() != 1 {
            return Err(crate::error::ErrorKind::State {
                current: "setup_changed",
            }
            .into());
        }
        super::start::store(&mut tx, access, fleet, &initial).await?;
        sqlx::query(sql::CLOSE_DRAFTS)
            .bind(fleet.as_str())
            .bind(DraftState::Closed.as_str())
            .bind(super::PRIVATE_RETENTION)
            .bind(DraftState::Saving.as_str())
            .execute(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        tx.commit().await.map_err(query(CONTEXT))?;
        // Only the commit whose guarded update moved `installing` to `active`
        // gets here; a retry finds the fleet active and returns above.
        let step = AuthoringStep::SetupFinished {
            fleet_id: fleet.clone(),
        };
        self.setup.drafts.reached(access, step);
        Ok(())
    }
}
