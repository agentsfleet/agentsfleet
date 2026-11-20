//! Sources are authorized and validated before reserving a fleet identity.

use super::{CONTEXT, Creations, sql};
use crate::{
    Access, Result,
    access::Person,
    candidate, draft,
    error::{ErrorKind, raise::query},
    review,
    types::{Candidate, SaveOrigin, SaveSource},
};
use af_core::id::Uuid7;
use sqlx::PgConnection;

pub(super) struct Captured {
    pub(super) origin: Option<SaveOrigin>,
    pub(super) candidate: Candidate,
    pub(super) context: review::Context,
    pub(super) draft: Option<draft::read::Locked>,
}

impl Creations {
    pub(super) async fn capture(
        &self,
        connection: &mut PgConnection,
        access: Access<'_>,
        person: &Person,
        source: &SaveSource,
    ) -> Result<Captured> {
        let SaveSource::Draft { draft_candidate_id } = source;
        let id: Option<Uuid7> = sqlx::query_scalar(sql::CANDIDATE_DRAFT)
            .bind(draft_candidate_id)
            .bind(access.workspace)
            .bind(&person.id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT))?;
        let id = id.ok_or(ErrorKind::Hidden)?;
        let locked = draft::read::lock(connection, access, person, &id).await?;
        if locked.head_candidate_id.as_ref() != Some(draft_candidate_id) {
            return Err(ErrorKind::State {
                current: "candidate_changed",
            }
            .into());
        }
        let candidate = candidate::read(connection, &id, draft_candidate_id).await?;
        Ok(Captured {
            origin: None,
            candidate,
            context: review::Context::from(&locked),
            draft: Some(locked),
        })
    }
}
