//! Late usage may settle; late text must pass current private authorization again.

use af_core::{error_code, id::Uuid7};
use af_wire::authoring::ModelUsage;
use sqlx::{Connection as _, PgConnection, Postgres, Transaction};

use super::super::{
    ModelCallInput, ModelCallResult, dispatch::CONTEXT, dispatch_status, provider::Decoded, sql,
};
use crate::{
    Drafts, Error, Result,
    error::{
        ErrorKind,
        raise::{query, stored},
    },
    funnel::Owed,
    lease::{Status, failure, guard},
};

pub(in crate::model) async fn finish(
    drafts: &Drafts,
    runner: &Uuid7,
    lease: &Uuid7,
    request: &garde::Valid<ModelCallInput>,
    counters: &guard::Counters,
    attempt: &guard::Attempt,
    received: super::Received,
) -> Result<ModelCallResult> {
    Completion {
        drafts,
        runner,
        lease,
        request,
        counters,
        attempt,
    }
    .save(received)
    .await
}

#[derive(Clone, Copy)]
struct Completion<'a> {
    drafts: &'a Drafts,
    runner: &'a Uuid7,
    lease: &'a Uuid7,
    request: &'a garde::Valid<ModelCallInput>,
    counters: &'a guard::Counters,
    attempt: &'a guard::Attempt,
}

impl Completion<'_> {
    async fn save(&self, received: super::Received) -> Result<ModelCallResult> {
        let Self {
            drafts, attempt, ..
        } = *self;
        let mut connection = drafts.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let mut owed = Owed::default();
        let denied = self.authorize(&mut tx, &mut owed).await?;
        let (decoded, rejection) = rejection(received.decoded);
        let decoded = match decoded {
            Ok(known) => known,
            Err(error) => {
                self.write(&mut tx, None, None, received.cause).await?;
                tx.commit().await.map_err(query(CONTEXT))?;
                owed.settle(&drafts.analytics);
                return Err(denied.unwrap_or_else(|| public_failure(error)));
            }
        };
        let denied = self
            .publish_context(&mut tx, received.compaction, &decoded, denied)
            .await?;
        let private = decoded.result.as_ref().filter(|_| denied.is_none());
        self.write(&mut tx, private, Some(&decoded), received.cause)
            .await?;
        if let Some(error) = &rejection {
            failure::record(&mut tx, attempt, error, &mut owed).await?;
        }
        tx.commit().await.map_err(query(CONTEXT))?;
        owed.settle(&drafts.analytics);
        if let Some(error) = denied.or(rejection) {
            return Err(error);
        }
        decoded.result.ok_or_else(|| ErrorKind::Provider.into())
    }

    /// Late output needs a fresh guard; denied private output still reaches accounting.
    async fn authorize(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        owed: &mut Owed,
    ) -> Result<Option<Error>> {
        let Self {
            drafts,
            runner,
            lease,
            counters,
            attempt,
            ..
        } = *self;
        let denied = match drafts.publication(tx, runner, lease, counters).await {
            Ok(_guarded) => return Ok(None),
            Err(error)
                if [
                    error_code::AUTHORING_NOT_FOUND,
                    error_code::AUTHORING_FENCE,
                    error_code::AUTHORING_MODEL,
                ]
                .contains(&error.code()) =>
            {
                error
            }
            Err(error) => return Err(error),
        };
        let locked: Option<Uuid7> = sqlx::query_scalar(sql::LOCK_ATTEMPT)
            .bind(&attempt.id)
            .bind(runner)
            .fetch_optional(&mut **tx)
            .await
            .map_err(query(CONTEXT))?;
        if locked.is_none() {
            return Err(ErrorKind::Hidden.into());
        }
        failure::record(tx, attempt, &denied, owed).await?;
        Ok(Some(denied))
    }

    async fn publish_context(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        compaction: Option<af_context::Compaction>,
        decoded: &Decoded,
        mut denied: Option<Error>,
    ) -> Result<Option<Error>> {
        let Self { attempt, .. } = *self;
        if denied.is_none()
            && let (Some(task), Some(summary)) = (compaction, decoded.summary.clone())
            && let Err(error) = super::super::context::publish(tx, attempt, task, summary).await
        {
            // A stale guarded publication withholds notes while preserving known usage.
            if error.code() == error_code::AUTHORING_FENCE {
                denied = Some(error);
            } else {
                return Err(error);
            }
        }
        Ok(denied)
    }
}

fn public_failure(error: Error) -> Error {
    if error.code() == error_code::AUTHORING_UNKNOWN {
        error
    } else {
        ErrorKind::Unknown.into()
    }
}

fn rejection(decoded: Result<Decoded>) -> (Result<Decoded>, Option<Error>) {
    match decoded {
        Err(error) if error.code() == error_code::AUTHORING_MODEL => {
            (Ok(Decoded::rejected()), Some(error))
        }
        other => (other, None),
    }
}

impl Completion<'_> {
    async fn write(
        &self,
        connection: &mut PgConnection,
        result: Option<&ModelCallResult>,
        decoded: Option<&Decoded>,
        cause: Option<crate::types::FailureCause>,
    ) -> Result<()> {
        let Self {
            attempt, request, ..
        } = *self;
        let status = match decoded {
            Some(decoded) if decoded.result.is_some() => dispatch_status::COMPLETED,
            Some(_) => dispatch_status::FAILED,
            None => dispatch_status::UNKNOWN,
        };
        let usage = decoded.map_or(&ModelUsage::Unknown, |decoded| &decoded.usage);
        let changed = sqlx::query(sql::COMPLETE)
            .bind(super::super::dispatch_id(request)?)
            .bind(&attempt.id)
            .bind(status)
            .bind(
                result
                    .map(serde_json::to_string)
                    .transpose()
                    .map_err(stored)?,
            )
            .bind(serde_json::to_string(usage).map_err(stored)?)
            .bind(dispatch_status::PENDING)
            .bind(dispatch_status::UNKNOWN)
            // The native reply is kept only beside a result it can replay.
            .bind(if result.is_some() {
                decoded.and_then(|value| value.continuation.as_ref())
            } else {
                None
            })
            .bind(cause.map(crate::types::FailureCause::as_str))
            .execute(&mut *connection)
            .await
            .map_err(query(CONTEXT))?;
        if changed.rows_affected() != 1 {
            return Err(ErrorKind::State { current: "settled" }.into());
        }
        sqlx::query(sql::METER)
            .bind(&attempt.id)
            .bind(decoded.map_or(0, |decoded| decoded.output))
            .bind(decoded.is_none())
            .bind(Status::AwaitingAccounting.as_str())
            .bind(dispatch_status::PENDING)
            .bind(dispatch_status::UNKNOWN)
            .execute(connection)
            .await
            .map_err(query(CONTEXT))?;
        Ok(())
    }
}
