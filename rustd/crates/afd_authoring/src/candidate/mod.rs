//! Immutable candidates are read or appended under the draft's row lock.

mod activity;
mod proposal;
mod receipt;
mod source;
mod sql;

use crate::{
    Result,
    error::raise::{query, stored},
    types::Candidate,
};
use afd_core::id::Uuid7;
use afd_fleet_runtime::definition::{Definition, POLICY_SEMANTICS, canonical_digest};
use sqlx::PgConnection;

pub(crate) use source::{Captured, resolve};
const CONTEXT: &str = "read or append immutable candidate";

/// [`crate::sql::candidate::READ`]'s row, by column name.
#[derive(sqlx::FromRow)]
struct Stored {
    id: String,
    revision: i64,
    definition: String,
    canonical_digest: String,
    policy_semantics: String,
}

pub(crate) async fn read(
    connection: &mut PgConnection,
    draft: &Uuid7,
    id: &Uuid7,
) -> Result<Candidate> {
    let row: Stored = sqlx::query_as(crate::sql::candidate::READ)
        .bind(id.as_str())
        .bind(draft.as_str())
        .fetch_one(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(Candidate {
        id: Uuid7::parse(&row.id)?,
        revision: row.revision.to_string(),
        definition: serde_json::from_str(&row.definition).map_err(stored)?,
        canonical_digest: row.canonical_digest,
        policy_semantics: row.policy_semantics,
    })
}

pub(crate) struct Replacement<'a> {
    pub(crate) id: &'a Uuid7,
    pub(crate) draft: &'a Uuid7,
    pub(crate) revision: i64,
    pub(crate) definition: &'a Definition,
}

pub(crate) async fn append(
    connection: &mut PgConnection,
    replacement: Replacement<'_>,
) -> Result<Candidate> {
    let Replacement {
        id,
        draft,
        revision,
        definition,
    } = replacement;
    let digest = canonical_digest(definition)?;
    let value = serde_json::to_string(definition).map_err(stored)?;
    sqlx::query(crate::sql::candidate::INSERT)
        .bind(id.as_str())
        .bind(draft.as_str())
        .bind(revision)
        .bind(value)
        .bind(&digest)
        .bind(POLICY_SEMANTICS)
        .execute(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    sqlx::query(crate::sql::candidate::SELECT_HEAD)
        .bind(draft.as_str())
        .bind(id.as_str())
        .execute(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(Candidate {
        id: id.clone(),
        revision: revision.to_string(),
        definition: definition.clone(),
        canonical_digest: digest,
        policy_semantics: POLICY_SEMANTICS.to_owned(),
    })
}
