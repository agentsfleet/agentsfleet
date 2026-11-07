//! Activating a tenant's own credential: one transaction, one decrypt.
//!
//! `PUT /v1/tenants/me/provider {mode:"self_managed"}` — the write ladder, from
//! the credential lock to the row the response echoes.
//!
//! # Why the whole ladder is one transaction
//!
//! Activation is a reference PRODUCER: it writes a registry entry naming a
//! stored credential, and points the selection row at that entry. `afd_vault`'s delete is
//! the destroyer. Between a probe and a commit taken separately, a delete would
//! leave the tenant's ACTIVE model pointing at a credential that no longer
//! exists — the worst instance of the orphan, because it breaks every later run
//! rather than one list row.
//!
//! Both sides take the credential's row lock FIRST, and that is the whole
//! serialization point: whoever reaches it first wins, and both outcomes are
//! correct. Producer first, and the delete blocks, then sees the new entry and
//! refuses. Delete first, and the producer blocks, then finds no credential and
//! answers [`Activation::CredentialMissing`] having written nothing.
//!
//! The correctness does NOT rest on "a write is a lock", which is too loose to
//! rely on: `ON CONFLICT DO NOTHING` waits on a unique conflict without
//! retaining a row lock on the tuple it lost to, and `DO UPDATE` takes `FOR NO
//! KEY UPDATE` on the row it finds. The arms behave differently, and neither
//! is the guarantee.
//!
//! It rests on the vault row alone. Every operation that can conflict over one
//! credential — this activation, the credential delete, the registry-entry
//! delete — locks that same `vault.secrets` tuple FIRST, so no two of them are
//! ever inside the entries or selection writes at once. Each then acquires
//! strictly forward in the treaty's order and never reaches back for something
//! it skipped, which is the no-deadlock condition however each lock is taken.
//!
//! The other two participants spell all three locks as explicit `FOR UPDATE`
//! reads because they READ to decide — the credential delete counts
//! references, the entry delete asks whether it is removing the active
//! selection. This side reads neither table to decide anything, so a pre-lock
//! would add ceremony without adding a guarantee: `SELECT … FOR UPDATE` on a
//! tenant with no selection row locks nothing at all, Postgres having no gap
//! locks, which is precisely the first-activation path.
//!
//! The protocol exists only because `secret_ref` is TEXT naming a
//! `(workspace_id, key_name)` pair rather than a foreign key. The selection's
//! pointer at the entry IS one, so that half the database already guards.
//!
//! # One decrypt, and none on the refusals a client provokes
//!
//! The outcome variants carry the error mapping out of the single transaction,
//! so the in-transaction read is the only read — nothing decrypts before the
//! lock to map errors early. The two credential
//! rungs are decided from `meta_provider` and `meta_has_key` on the row already
//! locked, so the most-walked refusals — a name nobody stored, a credential
//! that is not a provider key — never open an envelope at all.

use afd_core::id::Uuid7;
use afd_crypto::aad::Aad;
use afd_crypto::secret::SecretBytes;
use afd_vault::StoredEnvelope;
use jiff::Timestamp;
use sqlx::{Acquire as _, Transaction};

use crate::error::{Result, entropy_drained, query, row_malformed, vault_open};
use crate::provider::cap;
use crate::provider::endpoint::{OPENAI_COMPATIBLE, Rejection};
use crate::provider::selection::{COLUMN_ID, COLUMN_TENANT_MODEL, OwnModel, TABLE_WORKSPACES};
use crate::provider::store::Providers;
use crate::provider::vetted::{Refused, vet};
use crate::provider::{SecretKind, sql};

/// Statement name, for the context a query failure carries.
const CONTEXT_ACTIVATE: &str = "tenant provider activation";

/// The table a malformed pointer is reported against.
const TABLE_SELECTIONS: &str = "model.tenant_selections";

/// One [`sql::LOCK_CREDENTIAL_FOR_ACTIVATION`] row, by its column names.
#[derive(Debug, sqlx::FromRow)]
struct LockedCredential {
    meta_provider: Option<String>,
    meta_has_key: Option<bool>,
    workspace_id: String,
    #[sqlx(flatten)]
    envelope: StoredEnvelope,
}

/// One [`sql::ACTIVATE_TENANT_MODEL`] row: the pointer written and its ceiling.
#[derive(Debug, sqlx::FromRow)]
struct Pointed {
    tenant_model_id: String,
    context_cap_tokens: i32,
}

/// What an activation attempt resolved to.
///
/// Outcomes, not errors, and the distinction is load-bearing. Every variant
/// below is a decision made from a value this module already holds — a row
/// that is not there, metadata that does not describe a provider key, a
/// catalogue that does not carry the model. None of them is a fault, none
/// carries a cause, and folding them into [`crate::Error`] would make the
/// handler match on a datastore failure's neighbours to pick a registry code.
/// Genuine faults still travel as `Err`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Activation {
    /// Stored. Carries the entry the selection now points at, for the response
    /// to echo.
    Applied(OwnModel),
    /// This tenant has no workspace at all — a violated bootstrap invariant.
    NoWorkspace,
    /// No credential is held under that name.
    CredentialMissing,
    /// A credential is held, and its metadata does not describe a provider key.
    NotAProviderKey,
    /// The credential is held and will not read as a provider credential.
    Malformed,
    /// The credential's endpoint was refused, as the guard classified it.
    EndpointRefused(Rejection),
    /// No usable model: none named, or none the catalogue carries.
    ModelUnknown,
}

impl Providers {
    /// Activates `secret_ref` as this tenant's provider credential.
    ///
    /// `model` overrides the credential's own, which is the only thing that
    /// credential field is still read for. A blank or whitespace-padded model
    /// is [`Activation::ModelUnknown`] for every provider: the credential no
    /// longer guarantees a model, so this is the boundary that re-establishes
    /// "an activation must name a usable one". Padding is refused rather than
    /// trimmed, so the typo reaches the caller instead of being stored.
    ///
    /// `now` is the instant a registry entry's identifier is minted at; every
    /// row this writes is stamped by the database's own clock.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a stored envelope that
    /// will not open. Every refusal a client can provoke is an
    /// [`Activation`] variant instead.
    pub async fn activate(
        &self,
        tenant_id: &Uuid7,
        secret_ref: &str,
        model: Option<&str>,
        now: Timestamp,
    ) -> Result<Activation> {
        let mut connection = self.pool().acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_ACTIVATE))?;

        let Some(locked) =
            sqlx::query_as::<_, LockedCredential>(sql::LOCK_CREDENTIAL_FOR_ACTIVATION)
                .bind(tenant_id.as_str())
                .bind(secret_ref)
                .fetch_optional(&mut *transaction)
                .await
                .map_err(query(CONTEXT_ACTIVATE))?
        else {
            // Zero rows is two different facts. Telling them apart costs a
            // second read, spent only here — on a path nobody is waiting on.
            return Ok(
                if self.holds_a_workspace(&mut transaction, tenant_id).await? {
                    Activation::CredentialMissing
                } else {
                    Activation::NoWorkspace
                },
            );
        };

        if SecretKind::of(locked.meta_provider.as_deref(), locked.meta_has_key)
            != SecretKind::ProviderKey
        {
            return Ok(Activation::NotAProviderKey);
        }

        // From the locked row, not a second statement: this transaction already
        // holds a connection and the credential's row lock, and acquiring a
        // second connection under both is how a bounded pool starves under
        // concurrent activations. The join already resolved this exact row.
        let workspace = Uuid7::parse(&locked.workspace_id)
            .map_err(row_malformed(TABLE_WORKSPACES, COLUMN_ID))?;
        let opened = open_envelope(locked.envelope, &workspace, secret_ref, self.vault_key())?;
        let vetted = match vet(opened.expose()) {
            Ok(vetted) => vetted,
            Err(Refused::Malformed) => return Ok(Activation::Malformed),
            Err(Refused::Endpoint(rejection)) => {
                return Ok(Activation::EndpointRefused(rejection));
            }
        };

        let Some(effective) = effective_model(model, vetted.model.as_deref()) else {
            return Ok(Activation::ModelUnknown);
        };

        self.write_activation(
            transaction,
            tenant_id,
            secret_ref,
            &vetted.provider,
            effective,
            now,
        )
        .await
    }
}

/// Which model this activation stores, or nothing usable.
///
/// The override wins, and the credential's own is the fallback. A blank or
/// whitespace-padded name is not a model for either source.
fn effective_model<'a>(override_: Option<&'a str>, credential: Option<&'a str>) -> Option<&'a str> {
    let named = override_.or(credential)?;
    (!named.is_empty() && named.trim() == named).then_some(named)
}

/// Open the named storage projection under the caller's workspace and key name.
fn open_envelope(
    stored: StoredEnvelope,
    workspace: &Uuid7,
    name: &str,
    kek: &afd_crypto::secret::Kek,
) -> Result<SecretBytes> {
    stored
        .into_envelope()
        .map_err(vault_open)?
        .open(kek, &Aad::new(workspace.as_str(), name))
        .map_err(vault_open)
}

impl Providers {
    /// Whether `tenant_id` has a primary workspace at all, read in-transaction.
    ///
    /// The second half of telling two facts apart: a credential lock that
    /// matched nothing means the tenant has no workspace OR holds no such
    /// credential, and only this distinguishes them. Spent on the MISS path
    /// only, where the extra round trip costs nothing anyone is waiting on.
    ///
    /// Shared with the registry's create verb, which reaches the same fork off
    /// the same lock and owes its caller the same two refusals.
    pub(crate) async fn holds_a_workspace(
        &self,
        transaction: &mut Transaction<'_, sqlx::Postgres>,
        tenant_id: &Uuid7,
    ) -> Result<bool> {
        let held: Option<String> = sqlx::query_scalar(sql::SELECT_PRIMARY_WORKSPACE)
            .bind(tenant_id.as_str())
            .fetch_optional(&mut **transaction)
            .await
            .map_err(query(CONTEXT_ACTIVATE))?;

        Ok(held.is_some())
    }

    /// The registry entry and the gated selection pointer, then commit.
    async fn write_activation(
        &self,
        mut transaction: Transaction<'_, sqlx::Postgres>,
        tenant_id: &Uuid7,
        secret_ref: &str,
        provider: &str,
        model: &str,
        now: Timestamp,
    ) -> Result<Activation> {
        sqlx::query(sql::INSERT_TENANT_MODEL_IF_ABSENT)
            .bind(self.mint_entry_id(now)?.as_str())
            .bind(tenant_id.as_str())
            .bind(model)
            .bind(secret_ref)
            .execute(&mut *transaction)
            .await
            .map_err(query(CONTEXT_ACTIVATE))?;

        let pointed = sqlx::query_as::<_, Pointed>(sql::ACTIVATE_TENANT_MODEL)
            .bind(tenant_id.as_str())
            .bind(provider)
            .bind(model)
            .bind(secret_ref)
            .bind(provider == OPENAI_COMPATIBLE)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(query(CONTEXT_ACTIVATE))?;

        // No row means the gate refused: the catalogue does not carry this
        // (provider, model). The transaction is dropped without a commit, so
        // the registry entry above is rolled back with it.
        let Some(pointed) = pointed else {
            return Ok(Activation::ModelUnknown);
        };
        let own = OwnModel {
            tenant_model_id: Uuid7::parse(&pointed.tenant_model_id)
                .map_err(row_malformed(TABLE_SELECTIONS, COLUMN_TENANT_MODEL))?,
            model: model.into(),
            secret_ref: secret_ref.into(),
            provider: Some(provider.into()),
            context_cap_tokens: cap::stored(pointed.context_cap_tokens),
        };
        transaction
            .commit()
            .await
            .map_err(query(CONTEXT_ACTIVATE))?;

        Ok(Activation::Applied(own))
    }

    /// A fresh identifier for a registry entry, minted at `now`.
    pub(crate) fn mint_entry_id(&self, now: Timestamp) -> Result<Uuid7> {
        self.entropy().uuid7(now).map_err(entropy_drained)
    }
}

#[cfg(test)]
mod tests;
