//! Fund the next measured request outside locks, then revalidate before intent.
use super::{CONTEXT, ModelCallInput, ModelCalls, budget, capability, prepare};
use crate::{
    Result,
    error::{ErrorKind, raise::query},
    lease::guard,
};
use afd_core::id::Uuid7;
use sqlx::{Connection as _, PgConnection};

pub(super) struct Funding {
    prepared: prepare::Prepared,
    snapshot: super::super::context::Snapshot,
    ceiling: i64,
}

impl ModelCalls {
    pub(super) async fn funding(
        &self,
        runner: &Uuid7,
        lease: &Uuid7,
        request: &garde::Valid<ModelCallInput>,
        counters: &guard::Counters,
    ) -> Result<Option<Funding>> {
        let (mut attempt, funding) = {
            let mut connection = self.drafts.database.acquire().await?;
            let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
            let guarded = guard::bound(&mut tx, runner, lease, counters).await?;
            capability::check(&guarded.attempt, request)?;
            guard::current_model(&mut tx, &guarded).await?;
            if i64::from(request.call_index) != i64::from(guarded.attempt.call_count) + 1 {
                return Ok(None);
            }
            let continuations =
                super::super::continuation::read(&mut tx, &guarded.attempt, request, guarded.now)
                    .await?;
            let (prepared, snapshot) = super::super::context::prepare(
                &mut tx,
                &guarded.attempt,
                request,
                continuations,
                guarded.draft.head_candidate_id.is_some(),
            )
            .await?;
            let spent: i64 = sqlx::query_scalar(include_str!("spent.sql"))
                .bind(guarded.attempt.id.as_str())
                .bind(super::dispatch_status::KNOWN)
                .fetch_one(&mut *tx)
                .await
                .map_err(query(CONTEXT))?;
            let model = &guarded.attempt.model_snapshot;
            let input = if model.platform_managed {
                budget::cost(
                    model,
                    i64::try_from(prepared.input_tokens).map_err(|_overflow| ErrorKind::Balance)?,
                    0,
                )?
            } else {
                0
            };
            let ceiling = budget::bound(model)?
                .checked_add(spent)
                .and_then(|value| value.checked_add(input))
                .ok_or(ErrorKind::Balance)?
                .max(guarded.attempt.reserved_nanos);
            let funding = Funding {
                prepared,
                snapshot,
                ceiling,
            };
            tx.commit().await.map_err(query(CONTEXT))?;
            (guarded.attempt, funding)
        };
        attempt.reserved_nanos = funding.ceiling;
        budget::hold(self.drafts.biller.as_ref(), &attempt).await?;
        Ok(Some(funding))
    }
}

impl Funding {
    pub(super) async fn verify(
        self,
        connection: &mut PgConnection,
        guarded: &guard::Guarded,
        request: &garde::Valid<ModelCallInput>,
    ) -> Result<prepare::Prepared> {
        self.snapshot.verify(connection, guarded, request).await?;
        sqlx::query(include_str!("fund.sql"))
            .bind(guarded.attempt.id.as_str())
            .bind(self.ceiling)
            .execute(connection)
            .await
            .map_err(query(CONTEXT))?;
        Ok(self.prepared)
    }
}
