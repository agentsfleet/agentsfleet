//! A conditional metadata edit cannot alter a reusable native snapshot.

use super::{PLATFORM, PLATFORM_DRAFT, project, read::lock};
use crate::{
    Result, Setup,
    error::{
        ErrorKind,
        raise::{query, stored},
    },
    library::{CONTEXT, read},
    types::{PlatformLibrary, PlatformPatch},
};
use afd_core::clock::UnixMillis;
use sqlx::Connection as _;

const UPDATE: &str = "\
UPDATE core.fleet_libraries SET name=coalesce($2,name),description=coalesce($3,description),
credential_reasons=coalesce($4::jsonb,credential_reasons),visibility=coalesce($5,visibility),
resource_version=resource_version+1,updated_at=$6
WHERE id=$1::uuid AND (
name IS DISTINCT FROM coalesce($2,name) OR description IS DISTINCT FROM coalesce($3,description)
OR credential_reasons IS DISTINCT FROM coalesce($4::jsonb,credential_reasons)
OR visibility IS DISTINCT FROM coalesce($5,visibility))";

impl Setup {
    /// Curates, explicitly publishes or withdraws a platform entry.
    ///
    /// # Errors
    /// Requires platform scope at the adapter and a current strong resource tag.
    pub async fn curate_platform_library(
        &self,
        slug: &str,
        expected: Option<&str>,
        request: PlatformPatch,
        now: UnixMillis,
    ) -> Result<PlatformLibrary> {
        let request = garde::Unvalidated::new(request).validate()?;
        if request.name.is_none()
            && request.description.is_none()
            && request.required_credentials_reasons.is_none()
            && request.published.is_none()
        {
            return Err(ErrorKind::Invalid {
                detail: "Send at least one platform Library change",
            }
            .into());
        }
        crate::library::read::valid_id(slug)?;
        let expected = expected.ok_or(ErrorKind::Precondition)?;
        let mut connection = self.drafts.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let entry = lock(&mut tx, slug).await?;
        super::require_tag(&entry, expected)?;
        let reasons = request
            .required_credentials_reasons
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(stored)?;
        sqlx::query(UPDATE)
            .bind(entry.id.as_str())
            .bind(&request.name)
            .bind(&request.description)
            .bind(reasons)
            .bind(
                request
                    .published
                    .map(|published| if published { PLATFORM } else { PLATFORM_DRAFT }),
            )
            .bind(now.as_millis())
            .execute(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        let entry = lock(&mut tx, slug).await?;
        let head = read::revision(&mut tx, &entry.id, &entry.head_revision_id).await?;
        let result = project(&entry, &head);
        tx.commit().await.map_err(query(CONTEXT))?;
        Ok(result)
    }
}
