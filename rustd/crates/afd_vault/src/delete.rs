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
//! `secret_reference_txn.zig` carries a `Txn` with a boolean and an `abort`
//! that has to be idempotent, and its own module comment warns that `errdefer`
//! is the wrong tool because every handler holding one returns `void` — two
//! call sites had a rollback that was decoration. [`sqlx::Transaction`] rolls
//! back when it is DROPPED, so every early return here rolls back by the
//! language's rules rather than by a discipline each caller has to keep. There
//! is no flag, no idempotent abort, and no path that can forget.

use afd_core::id::Uuid7;
use sqlx::{Acquire as _, Row as _};

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
        // Rolls back on drop. Every `?` below is therefore a rollback, with no
        // guard to write and none to forget.
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_DELETE))?;
        let unreadable = query(CONTEXT_DELETE);

        // Step 1 — the credential itself. Zero rows means a concurrent
        // transaction reached the vault row first and removed it, which is
        // exactly what this request wanted: absence is the outcome, not a
        // failure. Every OTHER participant treats the same answer as fatal to
        // its own write, and that asymmetry is the protocol working.
        let held = sqlx::query(sql::LOCK_SECRET)
            .bind(workspace.as_str())
            .bind(name.as_str())
            .fetch_optional(&mut *transaction)
            .await
            .map_err(&unreadable)?;
        if held.is_none() {
            return Ok(Deleted::AlreadyAbsent);
        }

        // Step 0, issued here because step 1 is the cheaper rejection: no point
        // resolving an owner for a credential that is already gone. Whose
        // entries are at stake is a property of the CREDENTIAL, never of the
        // requester — see [`sql::OWNING_TENANT`].
        let tenant: String = sqlx::query(sql::OWNING_TENANT)
            .bind(workspace.as_str())
            .fetch_optional(&mut *transaction)
            .await
            .map_err(&unreadable)?
            .ok_or_else(|| crate::Error::from(ErrorKind::WorkspaceUnknown))?
            .try_get(0)
            .map_err(&unreadable)?;

        // Step 2 — every entry naming it, locked in id order and counted by the
        // same statement that locked them, so no entry can appear or vanish
        // between deciding and deleting. The identifiers are never decoded:
        // the row count IS the answer, and decoding them would allocate a
        // string per entry to drop unread.
        let entries = sqlx::query(sql::LOCK_ENTRIES)
            .bind(&tenant)
            .bind(name.as_str())
            .fetch_all(&mut *transaction)
            .await
            .map_err(&unreadable)?;
        if !entries.is_empty() {
            // Refused BEFORE step 3, where `secret_reference_txn.zig` takes all
            // three locks and lets its caller decide afterwards. Skipping a
            // LATER lock cannot introduce a deadlock — a cycle needs two
            // transactions holding what the other wants, and this one can never
            // hold the selection row — while taking it would hold a tenant-wide
            // lock for the length of a transaction that is about to roll back.
            return Err(still_referenced(entry_count(entries.len())));
        }

        // Step 3 — the tenant's active selection. Locked even though this path
        // does not write it: activation and deletion both read it to decide,
        // and a decision made against an unlocked row is one made against a row
        // that can change before the commit. Zero rows is normal.
        sqlx::query(sql::LOCK_SELECTION)
            .bind(&tenant)
            .fetch_all(&mut *transaction)
            .await
            .map_err(&unreadable)?;

        let removed = sqlx::query(sql::DELETE_SECRET)
            .bind(workspace.as_str())
            .bind(name.as_str())
            .execute(&mut *transaction)
            .await
            .map_err(&unreadable)?;
        transaction.commit().await.map_err(&unreadable)?;

        tracing::info!(
            workspace = workspace.as_str(),
            name = name.as_str(),
            event = "secret_deleted",
        );
        // The row was locked at step 1, so this is one under the same
        // transaction rather than a hopeful read of `rows_affected`.
        Ok(if removed.rows_affected() > 0 {
            Deleted::Removed
        } else {
            Deleted::AlreadyAbsent
        })
    }
}

/// The reference count, bounded to what the refusal can report.
///
/// A tenant with more registry entries than a `u32` can count is not a
/// condition this daemon can reach, and saturating is the honest answer to a
/// number whose only use is a sentence an operator reads.
fn entry_count(rows: usize) -> u32 {
    u32::try_from(rows).unwrap_or(u32::MAX)
}
