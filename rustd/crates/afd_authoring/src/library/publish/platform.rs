//! Platform staging captures a creator-owned native candidate, never a file.

use super::persist;
use crate::{
    Access, Result, Setup, access,
    error::{ErrorKind, raise::query},
    library::{CONTEXT, PLATFORM_DRAFT, platform, source},
    replay,
    types::{PublishLibrary, Published},
};
use afd_core::{clock::UnixMillis, id::Uuid7};
use sqlx::{Connection as _, PgConnection};

const INSERT: &str = "INSERT INTO core.fleet_libraries(id,tenant_id,visibility,name,description,head_revision_id,resource_version,created_at,updated_at,platform_slug) VALUES($1::uuid,NULL,$2,$3,$4,$5::uuid,1,$6,$6,$7) ON CONFLICT(platform_slug) WHERE platform_slug IS NOT NULL DO NOTHING RETURNING id::text";

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
        now: UnixMillis,
    ) -> Result<Published> {
        let request = garde::Unvalidated::new(request).validate()?;
        let mut connection = self.drafts.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let person = access::current(&mut tx, access).await?;
        let scope = format!("{}:platform-library:create:{}", access.workspace, person.id);
        let digest = replay::digest(&*request)?;
        let existing = replay::find(&mut tx, &scope, operation, &person.id, &digest).await?;
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
                now,
            };
            if replay::reserve(&mut tx, reservation).await? {
                let library = self.drafts.mint(now)?;
                let slug = candidate.definition.runtime().name().as_str();
                insert(
                    &mut tx,
                    &library,
                    &revision,
                    &request.label,
                    &candidate,
                    now,
                )
                .await?;
                persist::append(
                    &mut tx,
                    persist::NewRevision {
                        library: &library,
                        public_id: slug,
                        id: &revision,
                        number: 1,
                    },
                    &draft.id,
                    &candidate,
                    now,
                )
                .await?;
                platform::read::receipt(&mut tx, &revision).await?
            } else {
                let revision = replay::find(&mut tx, &scope, operation, &person.id, &digest)
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
    now: UnixMillis,
) -> Result<()> {
    let id: Option<String> = sqlx::query_scalar(INSERT)
        .bind(library.as_str())
        .bind(PLATFORM_DRAFT)
        .bind(label)
        .bind(candidate.definition.description())
        .bind(revision.as_str())
        .bind(now.as_millis())
        .bind(candidate.definition.runtime().name().as_str())
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?;
    id.ok_or(ErrorKind::State {
        current: "slug_exists",
    })?;
    Ok(())
}
