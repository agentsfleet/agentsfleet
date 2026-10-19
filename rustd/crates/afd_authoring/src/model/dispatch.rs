//! Commit the physical call identity before crossing the provider boundary.

use afd_core::id::Uuid7;
use afd_credential::provider::Providers;
use jiff::Timestamp;
use sqlx::{Connection as _, PgConnection, Postgres, Transaction};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;

use super::{
    ModelCallInput, ModelCallResult, Transport, budget, capability, dispatch_status, prepare,
    provider, receipt, sql,
};
use crate::{
    Drafts, Error, Result,
    error::{ErrorKind, raise::query},
    funnel::Owed,
    lease::{failure, guard},
    replay,
};

pub(super) const CONTEXT: &str = "reserve authoring model dispatch";

/// The daemon's scoped model invocation service, with a physical transport seam.
#[derive(Debug, Clone)]
pub struct ModelCalls {
    drafts: Drafts,
    providers: Providers,
    transport: Arc<dyn Transport>,
}

pub(super) struct Intent {
    pub(super) attempt: guard::Attempt,
    pub(super) outbound: provider::Outbound,
    pub(super) authorized_at: Instant,
    pub(super) output_limit: u32,
}

impl Intent {
    fn new(
        attempt: guard::Attempt,
        key: afd_crypto::secret::SecretString,
        prepared: prepare::Prepared,
        now: Timestamp,
        authorized_at: Instant,
    ) -> Self {
        let remaining = attempt
            .expires_at
            .duration_since(now)
            .try_into()
            .unwrap_or(Duration::ZERO);
        let outbound = provider::Outbound {
            key,
            body: prepared.body,
            timeout: remaining.min(budget::PROVIDER_TIMEOUT),
            inference: prepared.inference,
            model: attempt.model_snapshot.model.clone(),
            context_tokens: prepared.context_tokens,
        };
        Self {
            outbound,
            attempt,
            authorized_at,
            output_limit: prepared.output_limit,
        }
    }
}

pub(super) enum Decision {
    Send(Box<Intent>),
    Replay(ModelCallResult),
    Refused(Error),
}

impl ModelCalls {
    /// Shares the draft store and the daemon's key owner; runners receive neither.
    #[must_use]
    pub fn new(drafts: Drafts, providers: Providers, transport: Arc<dyn Transport>) -> Self {
        Self {
            drafts,
            providers,
            transport,
        }
    }

    /// Dispatches once or returns an authorized saved receipt.
    ///
    /// # Errors
    /// Refuses changed identities, invalid authority, token/money bounds and
    /// unresolved calls. An uncertain physical request is never resent.
    pub async fn call(
        &self,
        runner: &Uuid7,
        lease: &Uuid7,
        request: ModelCallInput,
    ) -> Result<ModelCallResult> {
        let request = prepare::request(request)?;
        let _attempt = Uuid7::parse(&request.attempt_id)?;
        let _dispatch = Uuid7::parse(&request.dispatch_id)?;
        let counters = guard::Counters::model_call(&request)?;
        let intent = match self.reserve(runner, lease, &request, &counters).await? {
            Decision::Replay(result) => return Ok(result),
            Decision::Refused(error) => return Err(error),
            Decision::Send(intent) => intent,
        };
        let Intent {
            attempt,
            mut outbound,
            authorized_at,
            output_limit,
        } = *intent;
        outbound.timeout = outbound.timeout.saturating_sub(authorized_at.elapsed());
        let decoded = if outbound.timeout.is_zero() {
            Err(ErrorKind::Unknown.into())
        } else {
            provider::complete(outbound, Arc::clone(&self.transport))
                .await
                .and_then(|(status, response)| {
                    provider::decode(
                        status,
                        &response,
                        &request,
                        &attempt.model_snapshot,
                        output_limit,
                    )
                })
        };
        receipt::finish(
            &self.drafts,
            runner,
            lease,
            &request,
            &counters,
            &attempt,
            decoded,
        )
        .await
    }

    async fn reserve(
        &self,
        runner: &Uuid7,
        lease: &Uuid7,
        request: &garde::Valid<ModelCallInput>,
        counters: &guard::Counters,
    ) -> Result<Decision> {
        let digest = replay::digest(&(
            &request.attempt_id,
            &request.fence,
            &request.dispatch_id,
            request.call_index,
            &request.request,
        ))?;
        let held = budget::held(&self.drafts, runner, lease).await;
        let mut connection = self.drafts.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let guarded = guard::bound(&mut tx, runner, lease, counters).await?;
        capability::check(&guarded.attempt, request)?;
        if guarded.attempt.usage_unknown
            || guarded.attempt.status == crate::lease::Status::AwaitingAccounting
        {
            return Err(ErrorKind::Unknown.into());
        }
        let attempt = guarded.attempt.clone();
        let result = async {
            guard::current_model(&mut tx, &guarded).await?;
            if let Some(prior) = receipt::prior(&mut tx, &guarded, request, &digest).await? {
                Ok(prior)
            } else {
                self.next(&mut tx, guarded, request, counters, &digest, held)
                    .await
            }
        }
        .await;
        let mut owed = Owed::default();
        let decision = match result {
            Ok(prior) => prior,
            Err(error) if failure::record(&mut tx, &attempt, &error, &mut owed).await? => {
                Decision::Refused(error)
            }
            Err(error) => return Err(error),
        };
        tx.commit().await.map_err(query(CONTEXT))?;
        owed.settle(&self.drafts.analytics);
        Ok(decision)
    }

    async fn next(
        &self,
        connection: &mut Transaction<'_, Postgres>,
        guarded: guard::Guarded,
        request: &garde::Valid<ModelCallInput>,
        counters: &guard::Counters,
        digest: &str,
        held: Result<()>,
    ) -> Result<Decision> {
        if guarded.attempt.call_count.checked_add(1).map(i64::from)
            != Some(i64::from(request.call_index))
        {
            return Err(ErrorKind::Replay.into());
        }
        if i64::from(request.request.max_output_tokens)
            > budget::output_limit(&guarded.attempt.model_snapshot)
                - guarded.attempt.output_token_count
        {
            return Err(ErrorKind::Token.into());
        }
        held?;
        let continuations =
            super::continuation::read(connection, &guarded.attempt, request, guarded.now).await?;
        let prepared = prepare::body(request, &guarded.attempt.model_snapshot, continuations)?;
        let key = self
            .providers
            .registered_key(
                connection,
                &guarded.attempt.tenant_id,
                &guarded.attempt.model_snapshot,
            )
            .await?
            .ok_or(ErrorKind::Model)?;
        let now = guard::refresh(connection, &guarded, counters).await?;
        let authorized_at = Instant::now();
        insert(connection, request, &guarded.attempt, digest).await?;
        Ok(Decision::Send(Box::new(Intent::new(
            guarded.attempt,
            key,
            prepared,
            now,
            authorized_at,
        ))))
    }
}

async fn insert(
    connection: &mut PgConnection,
    request: &garde::Valid<ModelCallInput>,
    attempt: &guard::Attempt,
    digest: &str,
) -> Result<()> {
    let inserted = sqlx::query(sql::INSERT)
        .bind(&request.dispatch_id)
        .bind(attempt.id.as_str())
        .bind(i64::from(request.call_index))
        .bind(digest)
        .bind(dispatch_status::PENDING)
        .bind(budget::PRIVATE_TTL)
        .execute(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    if inserted.rows_affected() != 1 {
        return Err(ErrorKind::Replay.into());
    }
    sqlx::query(sql::ADVANCE)
        .bind(attempt.id.as_str())
        .execute(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(())
}
