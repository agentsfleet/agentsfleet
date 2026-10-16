//! Late usage may settle; late text must pass current private authorization again.

use afd_core::{error_code, id::Uuid7};
use afd_wire::authoring::ModelUsage;
use sqlx::{Connection as _, PgConnection};

use super::super::{ModelCallInput, ModelCallResult, dispatch::CONTEXT, provider::Decoded, sql};
use crate::{
    Drafts, Error, Result,
    error::{
        ErrorKind,
        raise::{query, stored},
    },
    funnel::Owed,
    lease::{failure, guard},
};

pub(in crate::model) async fn finish(
    drafts: &Drafts,
    runner: &Uuid7,
    lease: &Uuid7,
    request: &garde::Valid<ModelCallInput>,
    counters: &guard::Counters,
    attempt: &guard::Attempt,
    decoded: Result<Decoded>,
) -> Result<ModelCallResult> {
    let mut connection = drafts.database.acquire().await?;
    let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
    let mut owed = Owed::default();
    let Publication { now, denied } =
        authorization(&mut tx, runner, lease, counters, attempt, &mut owed).await?;
    let decoded = match decoded {
        Ok(known) => known,
        Err(error) => {
            write(&mut tx, attempt, request, None, None, now).await?;
            tx.commit().await.map_err(query(CONTEXT))?;
            owed.settle(&drafts.analytics);
            return Err(denied.unwrap_or_else(|| {
                if error.code() == error_code::AUTHORING_UNKNOWN {
                    error
                } else {
                    ErrorKind::Unknown.into()
                }
            }));
        }
    };
    let private = if denied.is_none() {
        decoded.result.as_ref()
    } else {
        None
    };
    write(&mut tx, attempt, request, private, Some(&decoded), now).await?;
    tx.commit().await.map_err(query(CONTEXT))?;
    owed.settle(&drafts.analytics);
    if let Some(error) = denied {
        return Err(error);
    }
    decoded.result.ok_or_else(|| ErrorKind::Provider.into())
}

struct Publication {
    now: i64,
    denied: Option<Error>,
}

async fn authorization(
    connection: &mut PgConnection,
    runner: &Uuid7,
    lease: &Uuid7,
    counters: &guard::Counters,
    attempt: &guard::Attempt,
    owed: &mut Owed,
) -> Result<Publication> {
    let denied = match guard::publication(connection, runner, lease, counters).await {
        Ok(guarded) => {
            return Ok(Publication {
                now: guarded.now.as_millis(),
                denied: None,
            });
        }
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
    let locked: Option<String> = sqlx::query_scalar(sql::LOCK_ATTEMPT)
        .bind(attempt.id.as_str())
        .bind(runner.as_str())
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    if locked.is_none() {
        return Err(ErrorKind::Hidden.into());
    }
    failure::record(connection, attempt, &denied, owed).await?;
    let now = sqlx::query_scalar(crate::sql::CLOCK)
        .fetch_one(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(Publication {
        now,
        denied: Some(denied),
    })
}

async fn write(
    connection: &mut PgConnection,
    attempt: &guard::Attempt,
    request: &garde::Valid<ModelCallInput>,
    result: Option<&ModelCallResult>,
    decoded: Option<&Decoded>,
    now: i64,
) -> Result<()> {
    let status = match decoded {
        Some(decoded) if decoded.result.is_some() => "completed",
        Some(_) => "failed",
        None => "unknown",
    };
    let usage = decoded.map_or(&ModelUsage::Unknown, |decoded| &decoded.usage);
    let changed = sqlx::query(sql::COMPLETE)
        .bind(&request.dispatch_id)
        .bind(attempt.id.as_str())
        .bind(status)
        .bind(
            result
                .map(serde_json::to_string)
                .transpose()
                .map_err(stored)?,
        )
        .bind(serde_json::to_string(usage).map_err(stored)?)
        .bind(now)
        .execute(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    if changed.rows_affected() != 1 {
        return Err(ErrorKind::State { current: "settled" }.into());
    }
    sqlx::query(sql::METER)
        .bind(attempt.id.as_str())
        .bind(decoded.map_or(0, |decoded| decoded.input))
        .bind(decoded.map_or(0, |decoded| decoded.output))
        .bind(now)
        .bind(decoded.is_none())
        .execute(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(())
}
