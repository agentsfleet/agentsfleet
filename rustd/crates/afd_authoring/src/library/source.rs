//! Publication accepts only the authenticated creator's current idle candidate.

use super::{CONTEXT, sql};
use crate::{
    Access, Result,
    access::Person,
    candidate, draft,
    error::{ErrorKind, raise::query},
    types::Candidate,
};
use afd_core::id::Uuid7;
use afd_fleet_runtime::definition::POLICY_SEMANTICS;
use sqlx::PgConnection;

pub(super) async fn capture(
    connection: &mut PgConnection,
    access: Access<'_>,
    person: &Person,
    candidate: &Uuid7,
) -> Result<(draft::read::Locked, Candidate)> {
    let id: Option<String> = sqlx::query_scalar(sql::SOURCE)
        .bind(candidate.as_str())
        .bind(access.workspace.as_str())
        .bind(person.id.as_str())
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    let locked = draft::read::lock(
        connection,
        access,
        person,
        &Uuid7::parse(&id.ok_or(ErrorKind::Hidden)?)?,
    )
    .await?;
    if locked.head_candidate_id.as_ref() != Some(candidate) {
        return Err(ErrorKind::State {
            current: "candidate_changed",
        }
        .into());
    }
    let candidate = candidate::read(connection, &locked.id, candidate).await?;
    Ok((locked, candidate))
}

pub(super) async fn publishable(
    connection: &mut PgConnection,
    draft: &draft::read::Locked,
    candidate: &Candidate,
) -> Result<()> {
    draft::idle(connection, draft).await?;
    candidate.definition.validate_for_save()?;
    if candidate.policy_semantics != POLICY_SEMANTICS {
        return Err(ErrorKind::Policy.into());
    }
    Ok(())
}
