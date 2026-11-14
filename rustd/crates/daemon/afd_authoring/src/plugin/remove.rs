//! Deleting a reusable entry deletes its history; installed fleets keep their copies.

use super::{CONTEXT, read, sql};
use crate::{Access, Result, Setup, access, error::raise::query};
use af_core::id::Uuid7;
use sqlx::Connection as _;

impl Setup {
    /// Removes an owned entry and every revision it published.
    ///
    /// A fleet installed from it holds its own revision, and a draft started
    /// from or published to it keeps existing with that reference NULL.
    ///
    /// # Errors
    /// Hides foreign entries and preserves transaction failures.
    pub async fn delete_plugin(&self, access: Access<'_>, plugin: &Uuid7) -> Result<()> {
        let mut connection = self.drafts.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let person = access::current(&mut tx, access).await?;
        let entry = read::entry(&mut tx, &person.owner, plugin, true).await?;
        sqlx::query(sql::REMOVE)
            .bind(&entry.id)
            .execute(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        tx.commit().await.map_err(query(CONTEXT))?;
        Ok(())
    }
}
