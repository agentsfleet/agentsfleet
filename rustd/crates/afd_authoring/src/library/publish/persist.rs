//! Publication, immutable content and the private receipt commit together.

use crate::library::{CONTEXT, TENANT, sql};
use crate::{
    Result,
    error::raise::{query, stored},
    types::Candidate,
};
use afd_core::id::Uuid7;
use sqlx::PgConnection;

pub(super) async fn entry(
    connection: &mut PgConnection,
    owner: &Uuid7,
    library: &Uuid7,
    revision: &Uuid7,
    label: &str,
    candidate: &Candidate,
) -> Result<()> {
    sqlx::query(sql::INSERT)
        .bind(library.as_str())
        .bind(owner.as_str())
        .bind(TENANT)
        .bind(label)
        .bind(candidate.definition.description())
        .bind(revision.as_str())
        .bind(1_i64)
        .execute(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(())
}

pub(super) struct NewRevision<'a> {
    pub(super) library: &'a Uuid7,
    pub(super) id: &'a Uuid7,
    pub(super) number: i64,
}

pub(super) async fn append(
    connection: &mut PgConnection,
    revision: NewRevision<'_>,
    draft: &Uuid7,
    candidate: &Candidate,
) -> Result<()> {
    sqlx::query(sql::APPEND)
        .bind(revision.id.as_str())
        .bind(revision.library.as_str())
        .bind(revision.number)
        .bind(serde_json::to_string(&candidate.definition).map_err(stored)?)
        .bind(&candidate.canonical_digest)
        .bind(&candidate.policy_semantics)
        .execute(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    sqlx::query(sql::RECEIPT)
        .bind(draft.as_str())
        .bind(revision.id.as_str())
        .execute(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(())
}
