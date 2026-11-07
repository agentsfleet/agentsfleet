//! A conditional metadata edit cannot alter a reusable native snapshot.

use super::{PLATFORM, PLATFORM_DRAFT, project, read::lock};
use crate::{
    Result, Setup,
    error::{
        ErrorKind,
        raise::{query, stored},
    },
    library::{CONTEXT, read, sql},
    types::{PlatformLibrary, PlatformPatch},
};
use sqlx::Connection as _;

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
        sqlx::query(sql::CURATE)
            .bind(entry.id.as_str())
            .bind(&request.name)
            .bind(&request.description)
            .bind(reasons)
            .bind(
                request
                    .published
                    .map(|published| if published { PLATFORM } else { PLATFORM_DRAFT }),
            )
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
