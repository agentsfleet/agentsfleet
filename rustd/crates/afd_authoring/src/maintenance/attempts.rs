//! Wallet, draft and attempt locks match the live writer's ordering.

use super::sql;
use crate::{
    Drafts, Result,
    error::raise::query,
    funnel::Owed,
    lease::{
        Status,
        guard::{Attempt, AttemptRow},
    },
    types::{MessageState, RUNNING},
};
use sqlx::Connection as _;

const CONTEXT: &str = "recover interrupted authoring attempts";
/// What an expired attempt's message becomes while its usage is unknown.
const FAILED: &str = MessageState::Failed.as_str();
const LIMIT: i64 = 64;

/// [`sql::CANDIDATES`]'s row, by column name.
#[derive(sqlx::FromRow)]
struct Hint {
    id: String,
    tenant_id: String,
    draft_id: String,
}

pub(super) async fn expire(drafts: &Drafts) -> Result<u64> {
    let hints: Vec<Hint> = sqlx::query_as(sql::CANDIDATES)
        .bind(Status::Active.as_str())
        .bind(LIMIT)
        .fetch_all(&mut *drafts.database.acquire().await?)
        .await
        .map_err(query(CONTEXT))?;
    let mut count = 0;
    for hint in hints {
        count += u64::from(expire_one(drafts, &hint).await?);
    }
    Ok(count)
}

async fn expire_one(drafts: &Drafts, hint: &Hint) -> Result<bool> {
    let mut connection = drafts.database.acquire().await?;
    let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
    let _: Option<String> = sqlx::query_scalar(crate::lease::sql::WALLET)
        .bind(&hint.tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(query(CONTEXT))?;
    let _: Option<String> = sqlx::query_scalar(sql::DRAFT)
        .bind(&hint.draft_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(query(CONTEXT))?;
    let row: Option<AttemptRow> = sqlx::query_as(crate::lease::sql::LOCK_BY_ID)
        .bind(&hint.id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(query(CONTEXT))?;
    let Some(row) = row else { return Ok(false) };
    let attempt = row.decode()?;
    let now = crate::sql::clock(&mut tx, CONTEXT).await?;
    if attempt.status != Status::Active
        || (attempt.lease_expires_at > now && attempt.expires_at > now)
    {
        return Ok(false);
    }
    let mut owed = Owed::default();
    mark(&mut tx, &attempt, &mut owed).await?;
    tx.commit().await.map_err(query(CONTEXT))?;
    owed.settle(&drafts.analytics);
    Ok(true)
}

async fn mark(
    connection: &mut sqlx::PgConnection,
    attempt: &Attempt,
    owed: &mut Owed,
) -> Result<()> {
    sqlx::query(sql::EXPIRE)
        .bind(attempt.id.as_str())
        .bind(Status::AwaitingAccounting.as_str())
        .execute(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    let changed = sqlx::query(crate::sql::message::FAIL)
        .bind(attempt.draft_id.as_str())
        .bind(attempt.message_sequence)
        .bind(FAILED)
        .bind(crate::lease::report::verdict::reason(
            afd_wire::authoring::FailureReason::ProviderUsageUnknown,
        ))
        .bind(RUNNING)
        .execute(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    if changed.rows_affected() == 1 {
        owed.finished(connection, attempt.into(), FAILED).await?;
    }
    Ok(())
}
