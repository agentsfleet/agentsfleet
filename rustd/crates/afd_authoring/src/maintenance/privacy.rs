//! Closed draft retention and private dispatch results use database time.

use crate::{Drafts, Result, error::raise::query, types::DraftState};
use afd_core::{clock::UnixMillis, id::Uuid7};
use sqlx::Connection as _;

const CONTEXT: &str = "expire private authoring content";
const LIMIT: i64 = 64;
const INSTALLING: &str = "installing";
const CLOSED_DRAFTS: &str = const_format::concatcp!(
    "\
SELECT d.id::text FROM core.authoring_drafts d WHERE d.state=$1
AND d.expires_at<=",
    afd_db::statement::NOW_MS,
    "
AND NOT EXISTS(SELECT 1 FROM fleet.authoring_attempts a WHERE a.draft_id=d.id
AND a.lease_expires_at>",
    afd_db::statement::NOW_MS,
    ")
AND NOT EXISTS(SELECT 1 FROM core.fleets f WHERE f.id=d.linked_fleet_id AND f.status=$2)
ORDER BY d.expires_at,d.id LIMIT $3 FOR UPDATE OF d SKIP LOCKED",
);
const RESULTS: &str = const_format::concatcp!(
    "\
WITH expired AS (SELECT id FROM fleet.authoring_dispatches
WHERE private_expires_at<=",
    afd_db::statement::NOW_MS,
    "
AND (result IS NOT NULL OR native_reply IS NOT NULL)
ORDER BY private_expires_at,id LIMIT $1 FOR UPDATE SKIP LOCKED)
UPDATE fleet.authoring_dispatches d SET result=NULL,native_reply=NULL
FROM expired WHERE d.id=expired.id",
);

pub(super) async fn drafts(drafts: &Drafts) -> Result<u64> {
    let mut connection = drafts.database.acquire().await?;
    let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
    let ids: Vec<String> = sqlx::query_scalar(CLOSED_DRAFTS)
        .bind(DraftState::Closed.as_str())
        .bind(INSTALLING)
        .bind(LIMIT)
        .fetch_all(&mut *tx)
        .await
        .map_err(query(CONTEXT))?;
    let now: i64 = sqlx::query_scalar(crate::sql::CLOCK)
        .fetch_one(&mut *tx)
        .await
        .map_err(query(CONTEXT))?;
    for id in &ids {
        crate::privacy::purge(&mut tx, &Uuid7::parse(id)?, UnixMillis::from_millis(now)).await?;
    }
    tx.commit().await.map_err(query(CONTEXT))?;
    Ok(ids.len() as u64)
}

pub(super) async fn results(drafts: &Drafts) -> Result<u64> {
    let result = sqlx::query(RESULTS)
        .bind(LIMIT)
        .execute(&mut *drafts.database.acquire().await?)
        .await
        .map_err(query(CONTEXT))?;
    Ok(result.rows_affected())
}
