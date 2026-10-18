//! Platform staging captures a creator-owned native candidate, never a file.

use super::persist;
use crate::{
    Access, Result, Setup, access,
    error::{ErrorKind, raise::query},
    library::{CONTEXT, PLATFORM_DRAFT, platform, source, sql},
    replay,
    types::{PublishLibrary, Published},
};
use afd_core::id::Uuid7;
use jiff::Timestamp;
use sqlx::{Connection as _, PgConnection};

impl Setup {
    /// Stages one current private candidate for explicit platform publication.
    ///
    /// # Errors
    /// The adapter requires platform scope; source access additionally requires
    /// live workspace membership and the authenticated draft creator.
    pub async fn stage_platform_library(
        &self,
        access: Access<'_>,
        operation: &Uuid7,
        request: PublishLibrary,
        now: Timestamp,
    ) -> Result<Published> {
        let request = garde::Unvalidated::new(request).validate()?;
        let mut connection = self.drafts.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let person = access::current(&mut tx, access).await?;
        let scope = format!("{}:platform-library:create:{}", access.workspace, person.id);
        let digest = replay::digest(&*request)?;
        let existing = replay::find(
            &mut tx,
            &scope,
            replay::Resource::LibraryRevision,
            operation,
            &person.id,
            &digest,
        )
        .await?;
        let result = if let Some(revision) = existing {
            platform::read::receipt(&mut tx, &revision).await?
        } else {
            let (draft, candidate) =
                source::capture(&mut tx, access, &person, &request.draft_candidate_id).await?;
            source::publishable(&mut tx, &draft, &candidate).await?;
            let revision = self.drafts.mint(now)?;
            let reservation = replay::Reservation {
                receipt_id: &self.drafts.mint(now)?,
                workspace: access.workspace,
                person: &person,
                scope: &scope,
                operation,
                digest: &digest,
                resource: &revision,
            };
            if replay::reserve(&mut tx, reservation).await? {
                let library = self.drafts.mint(now)?;
                insert(&mut tx, &library, &revision, &request.label, &candidate).await?;
                persist::append(
                    &mut tx,
                    persist::NewRevision {
                        library: &library,
                        id: &revision,
                        number: 1,
                    },
                    &draft.id,
                    &candidate,
                )
                .await?;
                platform::read::receipt(&mut tx, &revision).await?
            } else {
                let revision = replay::find(
                    &mut tx,
                    &scope,
                    replay::Resource::LibraryRevision,
                    operation,
                    &person.id,
                    &digest,
                )
                .await?
                .ok_or(ErrorKind::Gone)?;
                platform::read::receipt(&mut tx, &revision).await?
            }
        };
        tx.commit().await.map_err(query(CONTEXT))?;
        Ok(result)
    }
}

async fn insert(
    connection: &mut PgConnection,
    library: &Uuid7,
    revision: &Uuid7,
    label: &str,
    candidate: &crate::types::Candidate,
) -> Result<()> {
    let id: Option<String> = sqlx::query_scalar(sql::INSERT_PLATFORM)
        .bind(library.as_str())
        .bind(PLATFORM_DRAFT)
        .bind(label)
        .bind(candidate.definition.description())
        .bind(revision.as_str())
        .bind(1_i64)
        .bind(candidate.definition.runtime().name().as_str())
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?;
    id.ok_or(ErrorKind::State {
        current: "slug_exists",
    })?;
    Ok(())
}
