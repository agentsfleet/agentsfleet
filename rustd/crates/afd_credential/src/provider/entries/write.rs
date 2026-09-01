//! Adding, retargeting and removing a registry entry.
//!
//! # Two of the three take the credential's row lock, and one does not
//!
//! Adding is a reference PRODUCER and removing is a reference DESTROYER, so
//! both participate in the treaty [`crate::provider::activate`] describes:
//! `secret_ref` is TEXT rather than a foreign key, the database cannot refuse
//! an orphaning delete on its own, and every participant takes the
//! `vault.secrets` row lock FIRST. That lock is the serialization point;
//! whoever reaches it first wins and both outcomes are correct.
//!
//! Retargeting takes nothing. It changes which MODEL an entry names and never
//! its credential, so no reference is created or destroyed and there is no
//! second participant to serialize against — a lock there would be ceremony
//! without a guarantee.
//!
//! # Rollback is the language's, not a discipline
//!
//! `secret_reference_txn.zig` carries an open flag and an idempotent `abort`,
//! and its own comment warns that `errdefer` is the wrong tool because the
//! handlers holding one return `void` — two call sites had a rollback that
//! never ran. A [`sqlx::Transaction`] rolls back when it is DROPPED, so every
//! early return below rolls back by the language's rules. There is no flag and
//! no path that can forget.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use sqlx::{Acquire as _, Transaction};

use super::page::read_entry;
use super::sql;
use super::{Added, Entry, Removed, Retargeted, is_active};
use crate::error::{Result, query};
use crate::provider::selection::read_selection;
use crate::provider::sql as provider_sql;
use crate::provider::store::Providers;

/// The context a failed registry write reports under.
const CONTEXT_WRITE: &str = "tenant model registry write";

impl Providers {
    /// Adds `model_id` on `secret_ref` to `tenant`'s registry.
    ///
    /// The credential is locked before it is decided to exist, so the decision
    /// and the insert are one act — a concurrent credential delete cannot land
    /// in the gap and leave this entry pointing at nothing.
    ///
    /// # Errors
    /// Reports a datastore that would not answer and a host that cannot draw
    /// the entropy an id is minted from. Every refusal a client can provoke is
    /// an [`Added`] variant instead.
    pub async fn add_entry(
        &self,
        tenant: &Uuid7,
        model_id: &str,
        secret_ref: &str,
        now: UnixMillis,
    ) -> Result<Added> {
        let mut connection = self.pool().acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_WRITE))?;

        let locked = sqlx::query(provider_sql::LOCK_CREDENTIAL_FOR_REFERENCE)
            .bind(tenant.as_str())
            .bind(secret_ref)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(query(CONTEXT_WRITE))?;
        if locked.is_none() {
            // Zero rows is two different facts, and the same fork activation
            // reaches off the same lock.
            return Ok(if self.holds_a_workspace(&mut transaction, tenant).await? {
                Added::CredentialMissing
            } else {
                Added::NoWorkspace
            });
        }

        let stored = sqlx::query(sql::INSERT_ENTRY)
            .bind(self.mint_entry_id(now)?.as_str())
            .bind(tenant.as_str())
            .bind(model_id)
            .bind(secret_ref)
            .bind(now.as_millis())
            .fetch_optional(&mut *transaction)
            .await
            .map_err(query(CONTEXT_WRITE))?;

        // No row back IS the duplicate: `DO NOTHING` returns nothing, and the
        // pair the tenant already holds is the only thing that can conflict.
        let Some(row) = stored else {
            return Ok(Added::Duplicate);
        };
        let entry = read_entry(&row)?;
        // Committed BEFORE the caller is told. A 201 whose transaction then
        // failed to commit is the worst outcome available: the client records
        // an id that does not exist.
        transaction.commit().await.map_err(query(CONTEXT_WRITE))?;
        Ok(Added::Stored(entry))
    }

    /// Points `entry_id` at a different model, keeping its credential.
    ///
    /// # Errors
    /// Reports a datastore that would not answer and a stored row this daemon
    /// cannot read. Both refusals are [`Retargeted`] variants.
    pub async fn set_entry_model(
        &self,
        tenant: &Uuid7,
        entry_id: &Uuid7,
        model_id: &str,
        now: UnixMillis,
    ) -> Result<Retargeted> {
        let mut connection = self.pool().acquire().await?;
        let updated = sqlx::query(sql::UPDATE_ENTRY_MODEL)
            .bind(entry_id.as_str())
            .bind(tenant.as_str())
            .bind(model_id)
            .bind(now.as_millis())
            .fetch_optional(&mut *connection)
            .await;

        match updated {
            Ok(Some(row)) => Ok(Retargeted::Stored(read_entry(&row)?)),
            Ok(None) => Ok(Retargeted::NotFound),
            // The domain key is `(tenant, model, secret_ref)`, so moving an
            // entry onto a model this credential already carries collides with
            // the tenant's OTHER row. `ON CONFLICT` cannot express that: the
            // statement would have to update a row it is not addressing.
            Err(unique) if is_unique_violation(&unique) => Ok(Retargeted::Duplicate),
            Err(other) => Err(query(CONTEXT_WRITE)(other)),
        }
    }

    /// Removes `entry_id`, unless it is what the tenant is running on.
    ///
    /// Idempotent: an id that does not resolve answers [`Removed::Done`],
    /// because a caller retrying a delete it never saw the response to wants
    /// the row gone and it is.
    ///
    /// # Errors
    /// Reports a datastore that would not answer and a stored row this daemon
    /// cannot read.
    pub async fn remove_entry(&self, tenant: &Uuid7, entry_id: &Uuid7) -> Result<Removed> {
        // Named before locked: the treaty starts at the vault row, so the entry
        // has to say which credential that is. Read outside the transaction
        // because a row that vanishes before the lock is one the delete below
        // finds already gone, which is the same answer.
        let Some(entry) = self.entry(tenant, entry_id).await? else {
            return Ok(Removed::Done);
        };

        let mut connection = self.pool().acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_WRITE))?;
        let held = sqlx::query(provider_sql::LOCK_CREDENTIAL_FOR_REFERENCE)
            .bind(tenant.as_str())
            .bind(&*entry.secret_ref)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(query(CONTEXT_WRITE))?;

        // The active check is SKIPPED when the credential is already gone, and
        // that is deliberate rather than an omission. What the check protects is
        // "the active selection always has a matching entry" — and a selection
        // naming a credential nobody holds has already lost the guarantee the
        // entry was carrying. Refusing here would stand on a broken invariant to
        // strand a row the tenant cannot remove and cannot activate: the entry
        // is an orphan, and removing it is the cleanup. There is no reference
        // race left to lose either, which is why nothing is held over it.
        if held.is_some()
            && self
                .selection_holds(&mut transaction, tenant, &entry)
                .await?
        {
            return Ok(Removed::Active);
        }

        sqlx::query(sql::DELETE_ENTRY)
            .bind(entry_id.as_str())
            .bind(tenant.as_str())
            .execute(&mut *transaction)
            .await
            .map_err(query(CONTEXT_WRITE))?;
        transaction.commit().await.map_err(query(CONTEXT_WRITE))?;
        Ok(Removed::Done)
    }

    /// One entry by id, or nothing if this tenant has no such row.
    ///
    /// # Errors
    /// Reports a datastore that would not answer and a stored row this daemon
    /// cannot read.
    pub async fn entry(&self, tenant: &Uuid7, entry_id: &Uuid7) -> Result<Option<Entry>> {
        let mut connection = self.pool().acquire().await?;
        sqlx::query(sql::SELECT_ENTRY)
            .bind(entry_id.as_str())
            .bind(tenant.as_str())
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_WRITE))?
            .as_ref()
            .map(read_entry)
            .transpose()
    }

    /// Whether the tenant's selection names `entry`, read under the lock.
    ///
    /// The check and the DELETE must be one act. They were once two
    /// unsynchronized statements, which let an activation commit in the gap:
    /// the check saw an inactive entry, activation made it the selection, and
    /// the delete removed the row the selection names — leaving an active
    /// selection with no registry entry.
    async fn selection_holds(
        &self,
        transaction: &mut Transaction<'_, sqlx::Postgres>,
        tenant: &Uuid7,
        entry: &Entry,
    ) -> Result<bool> {
        let row = sqlx::query(provider_sql::SELECT_TENANT_MODEL_SELECTION)
            .bind(tenant.as_str())
            .fetch_optional(&mut **transaction)
            .await
            .map_err(query(CONTEXT_WRITE))?;

        let chosen = row.as_ref().map(read_selection).transpose()?;
        Ok(is_active(entry, chosen.as_ref()))
    }
}

/// Whether a datastore failure is the domain key refusing a second row.
///
/// Asked of the DATABASE's own classification rather than of the message text:
/// `23505` is the SQLSTATE for a unique violation, and matching on a rendered
/// sentence would break the first time a locale or a driver version changed it.
fn is_unique_violation(error: &sqlx::Error) -> bool {
    matches!(error, sqlx::Error::Database(failure) if failure.is_unique_violation())
}
