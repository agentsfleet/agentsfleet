//! The write half of the tenant's selection row: back to the platform default.
//!
//! [`selection`](super::selection) reads the two rows that say WHERE a key is;
//! this is the statement that points a tenant back at the platform's. It is a
//! separate file for the reason [`store`](super::store) gives — sibling modules
//! add verbs to [`Providers`] in their own files — and because the read
//! module's whole subject is what resolution may assume before anything is
//! opened, which a write does not share.
//!
//! The other direction, pointing the selection at one of the tenant's own
//! models, is [`activate`](super::activate): it has a credential to lock and a
//! catalogue gate to pass, and a reset has neither.

use afd_core::id::Uuid7;

use crate::error::{Result, query};
use crate::provider::sql;
use crate::provider::store::Providers;

/// Statement name, for the context a query failure carries.
const CONTEXT_RESET: &str = "tenant model selection reset";

impl Providers {
    /// Points `tenant_id` back at the platform default.
    ///
    /// Stores a NULL pointer rather than deleting the row, and stamps it with
    /// the database's clock: the lifecycle columns are the store's to write.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn reset(&self, tenant_id: &Uuid7) -> Result<()> {
        let mut connection = self.pool().acquire().await?;
        sqlx::query(sql::RESET_TENANT_SELECTION)
            .bind(tenant_id.as_str())
            .execute(&mut *connection)
            .await
            .map_err(query(CONTEXT_RESET))?;

        Ok(())
    }
}
