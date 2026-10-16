//! Wallet, draft and attempt locks match the live writer's ordering.

use crate::{
    Drafts, Result,
    error::raise::query,
    funnel::Owed,
    lease::{Status, guard::Attempt},
    types::{MessageState, RUNNING},
};
use sqlx::Connection as _;

const CONTEXT: &str = "recover interrupted authoring attempts";
/// What an expired attempt's message becomes while its usage is unknown.
const FAILED: &str = MessageState::Failed.as_str();
const LIMIT: i64 = 64;
const CANDIDATES: &str = const_format::concatcp!(
    "\
SELECT id::text,tenant_id::text,draft_id::text FROM fleet.authoring_attempts
WHERE status=$1 AND LEAST(lease_expires_at,deadline)<=",
    afd_db::statement::NOW_MS,
    "
ORDER BY LEAST(lease_expires_at,deadline),id LIMIT $2",
);
const DRAFT: &str = "SELECT id::text FROM core.authoring_drafts WHERE id=$1::uuid FOR UPDATE";
const LOCK: &str =
    "SELECT to_jsonb(a)::text FROM fleet.authoring_attempts a WHERE id=$1::uuid FOR UPDATE";
const EXPIRE: &str =
    "UPDATE fleet.authoring_attempts SET status=$2,updated_at=$3 WHERE id=$1::uuid";
const MESSAGE: &str = "\
UPDATE core.authoring_messages SET state=$3,failure_reason=$4,finished_at=$5,updated_at=$5
WHERE draft_id=$1::uuid AND sequence=$2 AND state=$6";

pub(super) async fn expire(drafts: &Drafts) -> Result<u64> {
    let hints: Vec<(String, String, String)> = sqlx::query_as(CANDIDATES)
        .bind(Status::Active.as_str())
        .bind(LIMIT)
        .fetch_all(&mut *drafts.database.acquire().await?)
        .await
        .map_err(query(CONTEXT))?;
    let mut count = 0;
    for (id, tenant, draft) in hints {
        count += u64::from(expire_one(drafts, &id, &tenant, &draft).await?);
    }
    Ok(count)
}

async fn expire_one(drafts: &Drafts, id: &str, tenant: &str, draft: &str) -> Result<bool> {
    let mut connection = drafts.database.acquire().await?;
    let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
    let _: Option<String> = sqlx::query_scalar(crate::lease::sql::WALLET)
        .bind(tenant)
        .fetch_optional(&mut *tx)
        .await
        .map_err(query(CONTEXT))?;
    let _: Option<String> = sqlx::query_scalar(DRAFT)
        .bind(draft)
        .fetch_optional(&mut *tx)
        .await
        .map_err(query(CONTEXT))?;
    let raw: Option<String> = sqlx::query_scalar(LOCK)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(query(CONTEXT))?;
    let Some(raw) = raw else { return Ok(false) };
    let attempt: Attempt = crate::draft::value(&raw)?;
    let now: i64 = sqlx::query_scalar(crate::sql::CLOCK)
        .fetch_one(&mut *tx)
        .await
        .map_err(query(CONTEXT))?;
    if attempt.status != Status::Active
        || (attempt.lease_expires_at > now && attempt.deadline > now)
    {
        return Ok(false);
    }
    let mut owed = Owed::default();
    mark(&mut tx, &attempt, now, &mut owed).await?;
    tx.commit().await.map_err(query(CONTEXT))?;
    owed.settle(&drafts.analytics);
    Ok(true)
}

async fn mark(
    connection: &mut sqlx::PgConnection,
    attempt: &Attempt,
    now: i64,
    owed: &mut Owed,
) -> Result<()> {
    sqlx::query(EXPIRE)
        .bind(attempt.id.as_str())
        .bind(Status::AwaitingAccounting.as_str())
        .bind(now)
        .execute(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    let changed = sqlx::query(MESSAGE)
        .bind(attempt.draft_id.as_str())
        .bind(attempt.message_sequence)
        .bind(FAILED)
        .bind(crate::lease::report::verdict::reason(
            afd_wire::authoring::FailureReason::ProviderUsageUnknown,
        ))
        .bind(now)
        .bind(RUNNING)
        .execute(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    if changed.rows_affected() == 1 {
        owed.finished(connection, attempt.into(), FAILED).await?;
    }
    Ok(())
}
