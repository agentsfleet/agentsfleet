//! Withdrawal precedes deletion; installed fleets keep their own revisions.

use super::{PLATFORM, read::lock};
use crate::{
    Result, Setup,
    error::{ErrorKind, raise::query},
    library::{CONTEXT, sql},
};
use sqlx::Connection as _;

impl Setup {
    /// Deletes an explicitly withdrawn native platform entry.
    ///
    /// # Errors
    /// Refuses published entries, stale tags and dependency failures.
    pub async fn delete_platform_library(&self, slug: &str, expected: Option<&str>) -> Result<()> {
        crate::library::read::valid_id(slug)?;
        let expected = expected.ok_or(ErrorKind::Precondition)?;
        let mut connection = self.drafts.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let entry = lock(&mut tx, slug).await?;
        super::require_tag(&entry, expected)?;
        if entry.visibility == PLATFORM {
            return Err(ErrorKind::State {
                current: "published",
            }
            .into());
        }
        sqlx::query(sql::REMOVE)
            .bind(entry.id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        tx.commit().await.map_err(query(CONTEXT))?;
        Ok(())
    }
}
