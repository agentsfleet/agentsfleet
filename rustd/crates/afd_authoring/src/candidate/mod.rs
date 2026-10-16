//! Immutable candidates are read or appended under the draft's row lock.

mod activity;
mod proposal;
mod receipt;
mod source;

use crate::{
    Result,
    error::raise::{query, stored},
    sql,
    types::Candidate,
};
use afd_core::{clock::UnixMillis, id::Uuid7};
use afd_fleet_runtime::definition::{Definition, POLICY_SEMANTICS, canonical_digest};
use serde::Deserialize;
use sqlx::PgConnection;

pub(crate) use source::resolve;
const CONTEXT: &str = "read or append immutable candidate";

#[derive(Deserialize)]
struct Stored {
    id: Uuid7,
    revision: i64,
    definition: Definition,
    canonical_digest: String,
    policy_semantics: String,
}

pub(crate) async fn read(
    connection: &mut PgConnection,
    draft: &Uuid7,
    id: &Uuid7,
) -> Result<Candidate> {
    let value: String = sqlx::query_scalar(sql::candidate::READ)
        .bind(id.as_str())
        .bind(draft.as_str())
        .fetch_one(connection)
        .await
        .map_err(query(CONTEXT))?;
    let stored: Stored = serde_json::from_str(&value).map_err(stored)?;
    Ok(Candidate {
        id: stored.id,
        revision: stored.revision.to_string(),
        definition: stored.definition,
        canonical_digest: stored.canonical_digest,
        policy_semantics: stored.policy_semantics,
    })
}

pub(crate) struct Replacement<'a> {
    pub(crate) id: &'a Uuid7,
    pub(crate) draft: &'a Uuid7,
    pub(crate) revision: i64,
    pub(crate) definition: &'a Definition,
    pub(crate) now: UnixMillis,
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
        now,
    } = replacement;
    let digest = canonical_digest(definition)?;
    let value = serde_json::to_string(definition).map_err(stored)?;
    sqlx::query(sql::candidate::INSERT)
        .bind(id.as_str())
        .bind(draft.as_str())
        .bind(revision)
        .bind(value)
        .bind(&digest)
        .bind(POLICY_SEMANTICS)
        .bind(now.as_millis())
        .execute(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    sqlx::query(sql::candidate::SELECT_HEAD)
        .bind(draft.as_str())
        .bind(id.as_str())
        .bind(revision)
        .bind(now.as_millis())
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
