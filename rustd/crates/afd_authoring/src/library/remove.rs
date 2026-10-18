//! Deleting a reusable entry deletes its history; installed fleets keep their copies.

use super::{CONTEXT, read, sql};
use crate::{Access, Result, Setup, access, error::raise::query};
use sqlx::Connection as _;

impl Setup {
    /// Removes an owned entry and every revision it published.
    ///
    /// A fleet installed from it holds its own revision, and a draft started
    /// from or published to it keeps existing with that reference NULL.
    ///
    /// # Errors
    /// Hides platform/foreign entries and preserves transaction failures.
    pub async fn delete_library(&self, access: Access<'_>, library: &str) -> Result<()> {
        read::valid_id(library)?;
        let mut connection = self.drafts.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let person = access::current(&mut tx, access).await?;
        let entry = read::entry(&mut tx, &person.owner, library, true).await?;
        sqlx::query(sql::REMOVE)
            .bind(entry.id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        tx.commit().await.map_err(query(CONTEXT))?;
        Ok(())
    }
}
