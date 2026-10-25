//! Fund the next measured request outside locks, then revalidate before intent.
use super::{CONTEXT, ModelCallInput, ModelCalls, budget, capability, prepare};
use crate::{
    Result,
    error::{ErrorKind, raise::query},
    lease::guard,
};
use afd_core::id::Uuid7;
use afd_crypto::secret::SecretString;
use sha2::{Digest as _, Sha256};
use sqlx::{Connection as _, PgConnection};
use subtle::ConstantTimeEq as _;

pub(super) struct Funding {
    prepared: prepare::Prepared,
    snapshot: super::super::context::Snapshot,
    ceiling: i64,
    spent: i64,
    counting_key: Option<SecretString>,
}

impl ModelCalls {
    pub(super) async fn funding(
        &self,
        runner: &Uuid7,
        lease: &Uuid7,
        request: &garde::Valid<ModelCallInput>,
        counters: &guard::Counters,
    ) -> Result<Option<Funding>> {
        let (mut attempt, mut funding) = {
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
            let counting_key =
                counting_key(&self.providers, &mut tx, &guarded.attempt, &prepared).await?;
            let funding = Funding {
                prepared,
                snapshot,
                ceiling: 0,
                spent,
                counting_key,
            };
            tx.commit().await.map_err(query(CONTEXT))?;
            (guarded.attempt, funding)
        };
        funding
            .confirm(&attempt, Arc::clone(&self.transport))
            .await?;
        funding.ceiling = funding.ceiling(&attempt)?;
        attempt.reserved_nanos = funding.ceiling;
        budget::hold(self.drafts.biller.as_ref(), &attempt).await?;
        Ok(Some(funding))
    }
}

impl Funding {
    async fn confirm(
        &mut self,
        attempt: &guard::Attempt,
        physical: Arc<dyn super::Transport>,
    ) -> Result<()> {
        let Some(key) = &self.counting_key else {
            return Ok(());
        };
        let route =
            afd_inference::Configured::new(&self.prepared.inference, self.prepared.context_tokens)?;
        let transport =
            afd_inference::transport::HttpTransport::new(Arc::new(super::provider::Once {
                physical,
                timeout: std::time::Duration::from_secs(10),
            }));
        let model = &attempt.model_snapshot.model;
        let control = afd_inference::stream::TurnControl {
            correlation_id: attempt.id.to_string().into(),
            ..Default::default()
        };
        let mut count = route
            .confirm_input(
                model,
                key.expose(),
                &self.prepared.body,
                &transport,
                &control,
            )
            .await?;
        self.prepared.body =
            route.bound_counted_output(model, self.prepared.body.clone(), &mut count)?;
        self.prepared.input_tokens = count.tokens();
        self.prepared.output_limit =
            u32::try_from(self.prepared.body.max_tokens.unwrap_or_default())
                .map_err(|_overflow| ErrorKind::Token)?;
        if let Some(task) = &mut self.prepared.compaction {
            task.output_tokens = self.prepared.output_limit;
        }
        self.prepared.measurement = Some(count);
        Ok(())
    }

    fn ceiling(&self, attempt: &guard::Attempt) -> Result<i64> {
        let model = &attempt.model_snapshot;
        let input = if model.platform_managed {
            budget::cost(
                model,
                i64::try_from(self.prepared.input_tokens)
                    .map_err(|_overflow| ErrorKind::Balance)?,
                0,
            )?
        } else {
            0
        };
        Ok(budget::bound(model)?
            .checked_add(self.spent)
            .and_then(|value| value.checked_add(input))
            .ok_or(ErrorKind::Balance)?
            .max(attempt.reserved_nanos))
    }

    pub(super) async fn verify(
        self,
        connection: &mut PgConnection,
        guarded: &guard::Guarded,
        request: &garde::Valid<ModelCallInput>,
        key: &SecretString,
    ) -> Result<prepare::Prepared> {
        if self.counting_key.as_ref().is_some_and(|counted| {
            let counted = Sha256::digest(counted.expose().as_bytes());
            let current = Sha256::digest(key.expose().as_bytes());
            !bool::from(counted.ct_eq(&current))
        }) {
            return Err(ErrorKind::Fence.into());
        }
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

use std::sync::Arc;

async fn counting_key(
    providers: &afd_credential::provider::Providers,
    connection: &mut PgConnection,
    attempt: &guard::Attempt,
    prepared: &prepare::Prepared,
) -> Result<Option<SecretString>> {
    let route = afd_inference::Configured::new(&prepared.inference, prepared.context_tokens)?;
    if !route.needs_native_count(&attempt.model_snapshot.model, &prepared.body)? {
        return Ok(None);
    }
    let key = providers
        .registered_key(connection, &attempt.tenant_id, &attempt.model_snapshot)
        .await?
        .ok_or(ErrorKind::Model)?;
    super::provider::validate_key(prepared.inference.api, &key)?;
    Ok(Some(key))
}
