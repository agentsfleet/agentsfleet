//! The write half of the tenant's selection: back to the platform default.
//!
//! [`selection`](super::selection) reads the two rows that say WHERE a key is;
//! this is the statement that points a tenant back at the platform's. It is a
//! separate file for the reason [`store`](super::store) gives — sibling modules
//! add verbs to [`Providers`] in their own files — and because the read
//! module's whole subject is what resolution may assume before anything is
//! opened, which a write does not share.
//!
//! The other direction, selecting one of the tenant's own models, is
//! [`activate`](super::activate): it has a credential to lock and a catalogue
//! gate to pass, and a reset has neither.

use af_core::id::Uuid7;
use sqlx::Acquire as _;

use crate::error::{Result, query};
use crate::provider::sql;
use crate::provider::store::Providers;

/// Statement name, for the context a query failure carries.
const CONTEXT_RESET: &str = "tenant model selection reset";

impl Providers {
    /// Points `tenant_id` back at the platform default.
    ///
    /// Clears the selected flag under the tenant lock activation takes, so a
    /// concurrent activation's entry is either selected before the reset reads
    /// or after it commits; the stamp is the database's clock.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn reset(&self, tenant_id: &Uuid7) -> Result<()> {
        let mut connection = self.pool().acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_RESET))?;
        sqlx::query(sql::LOCK_TENANT_FOR_SELECTION)
            .bind(tenant_id)
            .execute(&mut *transaction)
            .await
            .map_err(query(CONTEXT_RESET))?;
        sqlx::query(sql::RESET_TENANT_SELECTION)
            .bind(tenant_id)
            .execute(&mut *transaction)
            .await
            .map_err(query(CONTEXT_RESET))?;
        transaction.commit().await.map_err(query(CONTEXT_RESET))?;

        Ok(())
    }
}
