//! Closed draft retention and private dispatch results use database time.

use super::sql;
use crate::{Drafts, Result, error::raise::query, types::DraftState};
use afd_core::id::Uuid7;
use sqlx::Connection as _;

const CONTEXT: &str = "expire private authoring content";
const LIMIT: i64 = 64;
const INSTALLING: &str = "installing";

pub(super) async fn drafts(drafts: &Drafts) -> Result<u64> {
    let mut connection = drafts.database.acquire().await?;
    let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
    let ids: Vec<String> = sqlx::query_scalar(sql::CLOSED_DRAFTS)
        .bind(DraftState::Closed.as_str())
        .bind(INSTALLING)
        .bind(LIMIT)
        .fetch_all(&mut *tx)
        .await
        .map_err(query(CONTEXT))?;
    for id in &ids {
        crate::privacy::purge(&mut tx, &Uuid7::parse(id)?).await?;
    }
    tx.commit().await.map_err(query(CONTEXT))?;
    Ok(ids.len() as u64)
}

pub(super) async fn results(drafts: &Drafts) -> Result<u64> {
    let result = sqlx::query(sql::RESULTS)
        .bind(LIMIT)
        .execute(&mut *drafts.database.acquire().await?)
        .await
        .map_err(query(CONTEXT))?;
    Ok(result.rows_affected())
}
