//! A funded physical intent commits before any decision credential is opened.

use afd_core::id::Uuid7;
use sqlx::Connection as _;

use super::{
    CONTEXT, ModelCallInput, ModelCalls, Prepared, prepare::record_fallback,
};
use crate::{
    Result,
    error::{ErrorKind, raise::query},
    lease::guard,
    model::budget,
};

impl ModelCalls {
    pub(super) async fn reserve_edit(
        &self,
        runner: &Uuid7,
        lease: &Uuid7,
        request: &garde::Valid<ModelCallInput>,
        counters: &guard::Counters,
        prepared: &Prepared,
    ) -> Result<bool> {
        let mut attempt = prepared.attempt.clone();
        attempt.reserved_nanos = attempt
            .reserved_nanos
            .checked_add(prepared.reserved_nanos)
            .ok_or(ErrorKind::Balance)?;
        let held = budget::hold(self.drafts.biller.as_ref(), &attempt).await;
        let mut connection = self.drafts.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let guarded = guard::publication(&mut tx, runner, lease, counters).await?;
        let selected = afd_runner::decision_assignment_on(&mut tx, runner).await?;
        let fresh = selected
            .is_some_and(|current| *current.assignment == prepared.snapshot.assignment)
            && guarded.draft.candidate_revision == prepared.candidate_revision;
        let disposition = if held.is_err() {
            "unavailable"
        } else {
            "stale"
        };
        if held.is_err() || !fresh {
            record_fallback(&mut tx, request, &guarded, disposition).await?;
            tx.commit().await.map_err(query(CONTEXT))?;
            return Ok(false);
        }
        let inserted = sqlx::query(include_str!("sql/reserve.sql"))
            .bind(&request.dispatch_id)
            .bind(attempt.id.as_str())
            .bind(attempt.tenant_id.as_str())
            .bind(attempt.workspace_id.as_str())
            .bind(i64::from(afd_wire::usage::DECISION_SEQ_BASE) + i64::from(request.call_index))
            .bind(prepared.candidate_revision)
            .bind(sqlx::types::Json(&prepared.snapshot))
            .bind(prepared.reserved_nanos)
            .bind(jiff_sqlx::Timestamp::from(prepared.snapshot.at))
            .execute(&mut *tx)
            .await
            .map_err(query(CONTEXT))?
            .rows_affected();
        sqlx::query(include_str!("../dispatch/fund.sql"))
            .bind(attempt.id.as_str())
            .bind(attempt.reserved_nanos)
            .execute(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        tx.commit().await.map_err(query(CONTEXT))?;
        Ok(inserted == 1)
    }
}
