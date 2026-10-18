//! An unfinished intent is never permission for another physical send.

use jiff::Timestamp;
use serde::Deserialize;
use sqlx::PgConnection;

use super::super::{
    ModelCallInput, budget,
    dispatch::{CONTEXT, Decision},
    dispatch_status, sql,
};
use crate::{
    Result,
    draft::{value, word},
    error::{ErrorKind, raise::query},
    lease::{Status, guard::Guarded},
};

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum State {
    Pending,
    Completed,
    Failed,
    Unknown,
}

/// [`sql::PRIOR`]'s and [`sql::UNFINISHED`]'s row, by column name.
#[derive(sqlx::FromRow)]
struct Receipt {
    id: String,
    sequence: i32,
    request_digest: String,
    status: String,
    result: Option<String>,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    created_at: Timestamp,
}

pub(in crate::model) async fn prior(
    connection: &mut PgConnection,
    guarded: &Guarded,
    request: &garde::Valid<ModelCallInput>,
    digest: &str,
) -> Result<Option<Decision>> {
    let prior: Option<Receipt> = sqlx::query_as(sql::PRIOR)
        .bind(guarded.attempt.id.as_str())
        .bind(&request.dispatch_id)
        .bind(i64::from(request.call_index))
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    if let Some(receipt) = prior {
        if receipt.id != request.dispatch_id
            || i64::from(receipt.sequence) != i64::from(request.call_index)
            || receipt.request_digest != digest
        {
            return Err(ErrorKind::Replay.into());
        }
        return existing(connection, guarded, receipt).await.map(Some);
    }
    let unfinished: Option<Receipt> = sqlx::query_as(sql::UNFINISHED)
        .bind(guarded.attempt.id.as_str())
        .bind(dispatch_status::COMPLETED)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    if let Some(receipt) = unfinished {
        return existing(connection, guarded, receipt).await.map(Some);
    }
    Ok(None)
}

async fn existing(
    connection: &mut PgConnection,
    guarded: &Guarded,
    receipt: Receipt,
) -> Result<Decision> {
    let error = match word::<State>(&receipt.status)? {
        State::Completed => {
            let mut result: super::super::ModelCallResult =
                value(&receipt.result.ok_or(ErrorKind::Gone)?)?;
            result.replayed = true;
            return Ok(Decision::Replay(result));
        }
        State::Failed => ErrorKind::Provider,
        State::Unknown => ErrorKind::Unknown,
        State::Pending
            if guarded.now
                < afd_core::clock::after(receipt.created_at, budget::PROVIDER_TIMEOUT) =>
        {
            ErrorKind::State {
                current: "dispatching",
            }
        }
        State::Pending => {
            sqlx::query(sql::UNKNOWN)
                .bind(&receipt.id)
                .bind(dispatch_status::UNKNOWN)
                .execute(&mut *connection)
                .await
                .map_err(query(CONTEXT))?;
            sqlx::query(sql::UNCERTAIN)
                .bind(guarded.attempt.id.as_str())
                .bind(Status::AwaitingAccounting.as_str())
                .execute(connection)
                .await
                .map_err(query(CONTEXT))?;
            ErrorKind::Unknown
        }
    };
    Ok(Decision::Refused(error.into()))
}
