//! Closed draft retention uses database time.

use super::sql;
use crate::{Drafts, Result, error::raise::query, types::DraftState};
use af_core::id::Uuid7;
use sqlx::Connection as _;

const CONTEXT: &str = "expire private setup content";
const LIMIT: i64 = 64;
const INSTALLING: &str = "installing";

pub(super) async fn drafts(drafts: &Drafts) -> Result<u64> {
    let mut connection = drafts.database.acquire().await?;
    let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
    let ids: Vec<Uuid7> = sqlx::query_scalar(sql::CLOSED_DRAFTS)
        .bind(DraftState::Closed.as_str())
        .bind(INSTALLING)
        .bind(LIMIT)
        .fetch_all(&mut *tx)
        .await
        .map_err(query(CONTEXT))?;
    for id in &ids {
        crate::privacy::purge(&mut tx, id).await?;
    }
    tx.commit().await.map_err(query(CONTEXT))?;
    Ok(u64::try_from(ids.len()).unwrap_or(u64::MAX))
}
