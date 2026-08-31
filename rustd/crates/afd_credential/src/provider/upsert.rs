//! The write half of the tenant's selection row.
//!
//! [`selection`](super::selection) reads the two rows that say WHERE a key is;
//! this is the one statement that PUTS one there. It is a separate file for the
//! reason [`store`](super::store) gives — sibling modules add verbs to
//! [`Providers`] in their own files — and because the read module's whole
//! subject is what resolution may assume before anything is opened, which a
//! write does not share.
//!
//! # The coherent pair is the boundary's job, not this one's
//!
//! [`Posture`] carries no payload, so a [`Selection`] can still spell the two
//! states that mean nothing: platform mode naming a credential, and
//! self-managed mode naming none. Both are refused ONCE, where a request
//! becomes a `Selection` — the tenant handler answers the second with its own
//! registry code and a sentence a client can act on. This module writes the
//! pair it is handed rather than re-deciding it, so there is exactly one place
//! that says what a coherent selection is.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;

use crate::error::{Result, query};
use crate::provider::cap;
use crate::provider::selection::Selection;
use crate::provider::sql;
use crate::provider::store::Providers;

/// Statement name, for the context a query failure carries.
const CONTEXT_UPSERT: &str = "tenant model selection write";

impl Providers {
    /// Writes `tenant_id`'s selection, last-write-wins on its single row.
    ///
    /// `now` is passed rather than read from a clock here, so the caller that
    /// already stamped the rest of its work uses one instant for all of it and
    /// a test can write a row at a time it chooses.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a context ceiling wider
    /// than the column holds.
    pub async fn upsert(
        &self,
        tenant_id: &Uuid7,
        selection: &Selection,
        now: UnixMillis,
    ) -> Result<()> {
        let mut connection = self.pool().acquire().await?;
        sqlx::query(sql::UPSERT_TENANT_MODEL_SELECTION)
            .bind(tenant_id.as_str())
            .bind(selection.posture.as_str())
            .bind(&*selection.provider)
            .bind(&*selection.model)
            .bind(cap::column(selection.context_cap_tokens)?)
            .bind(selection.secret_ref.as_deref())
            .bind(now.as_millis())
            .execute(&mut *connection)
            .await
            .map_err(query(CONTEXT_UPSERT))?;

        Ok(())
    }
}
