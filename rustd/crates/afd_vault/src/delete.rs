//! Removing a credential, under the lock protocol that keeps the model
//! registry from naming one that is gone.
//!
//! # The race this closes
//!
//! `core.tenant_model_entries.secret_ref` names a `vault.secrets` row but
//! cannot be a foreign key: `secret_ref` is TEXT and the vault's identity is
//! `(workspace_id, key_name)`, while an entry is keyed by tenant. Both sides
//! were once check-then-act with nothing held between the check and the act:
//!
//! ```text
//! DELETE /workspaces/{ws}/secrets/{name}   POST /tenants/me/models
//! ------------------------------------     -----------------------
//! referenced count -> 0, proceed
//!                                          secret exists? -> yes, proceed
//! DELETE FROM vault.secrets
//!                                          INSERT entry  <-- orphan
//! ```
//!
//! The orphan survives every later read — the list degrades it to an opaque
//! credential — so nothing reports it. It fails at the point of use, when a
//! fleet tries to run and cannot resolve a key.
//!
//! # Why the transaction needs no `open` flag
//!
//! [`sqlx::Transaction`] rolls back when it is DROPPED, so every early return
//! here rolls back by the language's rules rather than by a discipline each
//! caller has to keep. There is no flag, no idempotent abort, and no path that
//! can forget.

use afd_core::id::Uuid7;
use sqlx::{Acquire as _, Row as _, Transaction};

use crate::error::{ErrorKind, Result, query, still_referenced};
use crate::secret::SecretName;
use crate::{Directory, sql};

/// The context a failed delete reports under.
const CONTEXT_DELETE: &str = "delete a workspace secret";

/// What a delete found.
///
/// A value rather than a `bool`, because a caller reading `Ok(false)` at the
/// HTTP edge has to remember which way round it is — and both outcomes answer
/// 204, so nothing forces them to get it right.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Deleted {
    /// The row was there and is now gone.
    Removed,
    /// Nothing was held under that name. The request got what it wanted.
    AlreadyAbsent,
}

impl Directory {
    /// Removes the credential at `(workspace, name)`, if the registry allows it.
    ///
    /// Idempotent: a name this workspace does not hold answers
    /// [`Deleted::AlreadyAbsent`] rather than failing, and so does one a
    /// concurrent delete removed first. Both mean the same thing to the caller —
    /// the credential is not there — and the route answers 204 for either.
    ///
    /// # Errors
    /// Refuses a credential the tenant's model registry still names, reporting
    /// how many entries did. Reports a datastore that would not answer, and a
    /// workspace whose owning tenant does not resolve — which is a broken
    /// invariant rather than a race, since `workspace_id` is a NOT NULL foreign
    /// key, and must fail loudly rather than degrade to "no tenant, so no
    /// references": that reasoning is exactly what once let a delete run blind
    /// over live entries.
    pub async fn delete(&self, workspace: &Uuid7, name: &SecretName) -> Result<Deleted> {
        let mut connection = self.database.acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_DELETE))?;
        let deleted = self.delete_in(&mut transaction, workspace, name).await?;
        transaction.commit().await.map_err(query(CONTEXT_DELETE))?;
        if deleted == Deleted::Removed {
            let workspace_id = workspace.as_str();
            let secret_name = name.as_str();
            tracing::info!(
                workspace = workspace_id,
                name = secret_name,
                event = "secret_deleted",
            );
        }
        Ok(deleted)
    }

    /// [`Self::delete`] inside the caller's transaction.
    ///
    /// The secret's row lock is taken first, so a caller that writes other rows
    /// in the same transaction orders itself against every writer of this
    /// credential by that one lock. A connector's Disconnect does exactly that:
    /// the handle and the routing rows go together or not at all.
    ///
    /// # Errors
    /// As [`Self::delete`]. The transaction is the caller's to commit; dropping
    /// it rolls back, which is the property this seam is for.
    pub async fn delete_in(
        &self,
        transaction: &mut Transaction<'_, sqlx::Postgres>,
        workspace: &Uuid7,
        name: &SecretName,
    ) -> Result<Deleted> {
        if !secret_is_held(transaction, workspace, name).await? {
            return Ok(Deleted::AlreadyAbsent);
        }

        let tenant = owning_tenant(transaction, workspace).await?;
        let references = reference_count(transaction, &tenant, name).await?;
        if references > 0 {
            return Err(still_referenced(entry_count(references)));
        }

        let unreadable = query(CONTEXT_DELETE);
        sqlx::query(sql::LOCK_SELECTION)
            .bind(&tenant)
            .fetch_all(&mut **transaction)
            .await
            .map_err(&unreadable)?;

        let removed = sqlx::query(sql::DELETE_SECRET)
            .bind(workspace.as_str())
            .bind(name.as_str())
            .execute(&mut **transaction)
            .await
            .map_err(&unreadable)?;
        Ok(if removed.rows_affected() > 0 {
            Deleted::Removed
        } else {
            Deleted::AlreadyAbsent
        })
    }
}

async fn secret_is_held(
    transaction: &mut Transaction<'_, sqlx::Postgres>,
    workspace: &Uuid7,
    name: &SecretName,
) -> Result<bool> {
    Ok(sqlx::query(sql::LOCK_SECRET)
        .bind(workspace.as_str())
        .bind(name.as_str())
        .fetch_optional(&mut **transaction)
        .await
        .map_err(query(CONTEXT_DELETE))?
        .is_some())
}

async fn owning_tenant(
    transaction: &mut Transaction<'_, sqlx::Postgres>,
    workspace: &Uuid7,
) -> Result<String> {
    sqlx::query(sql::OWNING_TENANT)
        .bind(workspace.as_str())
        .fetch_optional(&mut **transaction)
        .await
        .map_err(query(CONTEXT_DELETE))?
        .ok_or_else(|| crate::Error::from(ErrorKind::WorkspaceUnknown))?
        .try_get(0)
        .map_err(query(CONTEXT_DELETE))
}

async fn reference_count(
    transaction: &mut Transaction<'_, sqlx::Postgres>,
    tenant: &str,
    name: &SecretName,
) -> Result<usize> {
    Ok(sqlx::query(sql::LOCK_ENTRIES)
        .bind(tenant)
        .bind(name.as_str())
        .fetch_all(&mut **transaction)
        .await
        .map_err(query(CONTEXT_DELETE))?
        .len())
}

/// The reference count, bounded to what the refusal can report.
///
/// A tenant with more registry entries than a `u32` can count is not a
/// condition this daemon can reach, and saturating is the honest answer to a
/// number whose only use is a sentence an operator reads.
fn entry_count(rows: usize) -> u32 {
    u32::try_from(rows).unwrap_or(u32::MAX)
}
