//! An unfinished intent is never permission for another physical send.

use afd_core::id::Uuid7;
use serde::Deserialize;
use serde_json::Value;
use sqlx::PgConnection;

use super::super::{
    ModelCallInput, budget,
    dispatch::{CONTEXT, Decision},
    sql,
};
use crate::{
    Result,
    draft::value,
    error::{
        ErrorKind,
        raise::{query, stored},
    },
    lease::guard::Guarded,
};

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum State {
    Pending,
    Completed,
    Failed,
    Unknown,
}

#[derive(Deserialize)]
struct Receipt {
    id: Uuid7,
    call_index: i32,
    request_digest: String,
    status: State,
    result: Option<Value>,
    created_at: i64,
}

pub(in crate::model) async fn prior(
    connection: &mut PgConnection,
    guarded: &Guarded,
    request: &garde::Valid<ModelCallInput>,
    digest: &str,
) -> Result<Option<Decision>> {
    let raw: Option<String> = sqlx::query_scalar(sql::PRIOR)
        .bind(guarded.attempt.id.as_str())
        .bind(&request.dispatch_id)
        .bind(i64::from(request.call_index))
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    if let Some(raw) = raw {
        let receipt: Receipt = value(&raw)?;
        if receipt.id.as_str() != request.dispatch_id
            || i64::from(receipt.call_index) != i64::from(request.call_index)
            || receipt.request_digest != digest
        {
            return Err(ErrorKind::Replay.into());
        }
        return existing(connection, guarded, receipt).await.map(Some);
    }
    let raw: Option<String> = sqlx::query_scalar(sql::UNFINISHED)
        .bind(guarded.attempt.id.as_str())
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    if let Some(raw) = raw {
        return existing(connection, guarded, value(&raw)?).await.map(Some);
    }
    Ok(None)
}

async fn existing(
    connection: &mut PgConnection,
    guarded: &Guarded,
    receipt: Receipt,
) -> Result<Decision> {
    let error = match receipt.status {
        State::Completed => {
            let mut result: super::super::ModelCallResult =
                serde_json::from_value(receipt.result.ok_or(ErrorKind::Gone)?).map_err(stored)?;
            result.replayed = true;
            return Ok(Decision::Replay(result));
        }
        State::Failed => ErrorKind::Provider,
        State::Unknown => ErrorKind::Unknown,
        State::Pending
            if guarded.now.as_millis() - receipt.created_at < budget::PROVIDER_TIMEOUT_MS =>
        {
            ErrorKind::State {
                current: "dispatching",
            }
        }
        State::Pending => {
            sqlx::query(sql::UNKNOWN)
                .bind(receipt.id.as_str())
                .bind(guarded.now.as_millis())
                .execute(&mut *connection)
                .await
                .map_err(query(CONTEXT))?;
            sqlx::query(sql::UNCERTAIN)
                .bind(guarded.attempt.id.as_str())
                .bind(guarded.now.as_millis())
                .execute(connection)
                .await
                .map_err(query(CONTEXT))?;
            ErrorKind::Unknown
        }
    };
    Ok(Decision::Refused(error.into()))
}
