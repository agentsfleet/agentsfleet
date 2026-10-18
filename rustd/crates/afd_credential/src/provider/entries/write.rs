//! Adding, retargeting and removing a registry entry.
//!
//! # Two of the three take the credential's row lock, and one does not
//!
//! Adding is a reference PRODUCER and removing is a reference DESTROYER, so
//! both participate in the protocol [`crate::provider::activate`] describes:
//! `secret_ref` is TEXT rather than a foreign key, the database cannot refuse
//! an orphaning credential delete on its own, and every participant takes the
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
//! A hand-kept open flag and an explicit `abort` is a rollback a call site can
//! forget to run. A [`sqlx::Transaction`] rolls back when it is DROPPED, so
//! every early return below rolls back by the language's rules. There is no
//! flag and no path that can forget.

use afd_core::id::Uuid7;
use afd_db::constraint::violates_unique;
use jiff::Timestamp;
use sqlx::{Acquire as _, Transaction};

use super::sql;
use super::{Added, Entry, EntryRow, Removed, Retargeted};
use crate::error::{Result, query};
use crate::provider::sql as provider_sql;
use crate::provider::store::Providers;

/// The entry's domain key, `(tenant, model, secret_ref)`: the one unique
/// constraint a retarget can collide with (`schema/1330_model_tenant_models.sql`).
const UQ_ENTRY_DOMAIN_KEY: &str = "uq_tenant_models_tenant_id_model_name_secret_ref";

/// The context a failed registry write reports under.
const CONTEXT_WRITE: &str = "tenant model registry write";

impl Providers {
    /// Adds `model_name` on `secret_ref` to `tenant`'s registry.
    ///
    /// The credential is locked before it is decided to exist, so the decision
    /// and the insert are one act — a concurrent credential delete cannot land
    /// in the gap and leave this entry pointing at nothing.
    ///
    /// `now` is the instant the entry's identifier is minted at; the row's
    /// stamps are the database's own clock.
    ///
    /// # Errors
    /// Reports a datastore that would not answer and a host that cannot draw
    /// the entropy an id is minted from. Every refusal a client can provoke is
    /// an [`Added`] variant instead.
    pub async fn add_entry(
        &self,
        tenant: &Uuid7,
        model_name: &str,
        secret_ref: &str,
        now: Timestamp,
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

        let stored = sqlx::query_as::<_, EntryRow>(sql::INSERT_ENTRY)
            .bind(self.mint_entry_id(now)?.as_str())
            .bind(tenant.as_str())
            .bind(model_name)
            .bind(secret_ref)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(query(CONTEXT_WRITE))?;

        // No row back IS the duplicate: `DO NOTHING` returns nothing, and the
        // pair the tenant already holds is the only thing that can conflict.
        let Some(row) = stored else {
            return Ok(Added::Duplicate);
        };
        let entry = row.into_entry()?;
        // Committed BEFORE the caller is told. A 201 whose transaction then
        // failed to commit is the worst outcome available: the client records
        // an id that does not exist.
        transaction.commit().await.map_err(query(CONTEXT_WRITE))?;
        Ok(Added::Stored(entry))
    }

    /// Points `entry_id` at a different model, keeping its credential.
    ///
    /// The entry is what a selection points AT, so retargeting the active
    /// entry changes the model the tenant's next run uses.
    ///
    /// # Errors
    /// Reports a datastore that would not answer and a stored row this daemon
    /// cannot read. Both refusals are [`Retargeted`] variants.
    pub async fn set_entry_model(
        &self,
        tenant: &Uuid7,
        entry_id: &Uuid7,
        model_name: &str,
    ) -> Result<Retargeted> {
        let mut connection = self.pool().acquire().await?;
        let updated = sqlx::query_as::<_, EntryRow>(sql::UPDATE_ENTRY_MODEL)
            .bind(entry_id.as_str())
            .bind(tenant.as_str())
            .bind(model_name)
            .fetch_optional(&mut *connection)
            .await;

        match updated {
            Ok(Some(row)) => Ok(Retargeted::Stored(row.into_entry()?)),
            Ok(None) => Ok(Retargeted::NotFound),
            // The domain key is `(tenant, model, secret_ref)`, so moving an
            // entry onto a model this credential already carries collides with
            // the tenant's OTHER row. `ON CONFLICT` cannot express that: the
            // statement would have to update a row it is not addressing.
            Err(unique) if violates_unique(&unique, UQ_ENTRY_DOMAIN_KEY) => {
                Ok(Retargeted::Duplicate)
            }
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
        // a tenant silently falling back to the platform default — the
        // selection's foreign key sets the pointer NULL when its entry goes —
        // and a selection naming a credential nobody holds cannot run anyway.
        // Refusing here would strand a row the tenant cannot remove and cannot
        // activate: the entry is an orphan, and removing it is the cleanup.
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
        sqlx::query_as::<_, EntryRow>(sql::SELECT_ENTRY)
            .bind(entry_id.as_str())
            .bind(tenant.as_str())
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_WRITE))?
            .map(EntryRow::into_entry)
            .transpose()
    }

    /// Whether the tenant's selection points at `entry`, read under the lock.
    ///
    /// The check and the DELETE must be one act. They were once two
    /// unsynchronized statements, which let an activation commit in the gap:
    /// the check saw an inactive entry, activation made it the selection, and
    /// the delete removed it. Under the credential lock no activation of this
    /// entry's credential can be inside the gap.
    async fn selection_holds(
        &self,
        transaction: &mut Transaction<'_, sqlx::Postgres>,
        tenant: &Uuid7,
        entry: &Entry,
    ) -> Result<bool> {
        let holds: Option<bool> = sqlx::query_scalar(provider_sql::SELECT_SELECTION_HOLDS)
            .bind(tenant.as_str())
            .bind(entry.id.as_str())
            .fetch_optional(&mut **transaction)
            .await
            .map_err(query(CONTEXT_WRITE))?;

        Ok(holds.unwrap_or_default())
    }
}
