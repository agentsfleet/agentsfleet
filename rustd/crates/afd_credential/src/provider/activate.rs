//! Activating a tenant's own credential: one transaction, one decrypt.
//!
//! `PUT /v1/tenants/me/provider {mode:"self_managed"}` — the write ladder, from
//! the credential lock to the row the response echoes.
//!
//! # Why the whole ladder is one transaction
//!
//! Activation is a reference PRODUCER: it writes a registry entry and a
//! selection row that both name a stored credential. `afd_vault`'s delete is
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
//! The treaty exists only because `secret_ref` is TEXT rather than a foreign
//! key. `docs/architecture/tenant_provider_v2.md` §V2-1 is the schema change
//! that deletes it, this module's lock, and both sides of the contract.
//!
//! # One decrypt, and none on the refusals a client provokes
//!
//! The Zig decrypts twice — once to map errors before locking, once inside the
//! transaction. Here the outcome variants carry that mapping out of the single
//! transaction, so the in-transaction read is the only read. The two credential
//! rungs are decided from `meta_provider` and `meta_has_key` on the row already
//! locked, so the most-walked refusals — a name nobody stored, a credential
//! that is not a provider key — never open an envelope at all.

use afd_core::clock::UnixMillis;
use afd_core::id::{ENTROPY_LEN, Uuid7};
use afd_crypto::aad::Aad;
use afd_crypto::envelope::Envelope;
use afd_crypto::secret::SecretBytes;
use sqlx::{Acquire as _, Row as _, Transaction};

use crate::error::{
    Result, entropy_drained, mint_failed, provider_no_workspace, query, vault_open,
};
use crate::provider::endpoint::{OPENAI_COMPATIBLE, Rejection};
use crate::provider::selection::Selection;
use crate::provider::store::Providers;
use crate::provider::{SecretKind, managed, sql};
use afd_billing::Posture;

/// Statement name, for the context a query failure carries.
const CONTEXT_ACTIVATE: &str = "tenant provider activation";

/// Where the envelope block starts in [`sql::LOCK_CREDENTIAL_FOR_ACTIVATION`].
///
/// The two metadata columns lead, then the six envelope components in
/// [`Envelope::from_parts`]' own order plus the version — the same block
/// [`crate::vault`] reads, at a different offset.
const ENVELOPE_AT: usize = 2;

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
    /// Stored. Carries the row as written, for the response to echo.
    Applied(Selection),
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
    /// # Errors
    /// Reports a datastore that would not answer, and a stored envelope that
    /// will not open. Every refusal a client can provoke is an
    /// [`Activation`] variant instead.
    pub async fn activate(
        &self,
        tenant_id: &Uuid7,
        secret_ref: &str,
        model: Option<&str>,
        now: UnixMillis,
    ) -> Result<Activation> {
        let mut connection = self.pool().acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_ACTIVATE))?;

        let Some(row) = sqlx::query(sql::LOCK_CREDENTIAL_FOR_ACTIVATION)
            .bind(tenant_id.as_str())
            .bind(secret_ref)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(query(CONTEXT_ACTIVATE))?
        else {
            // Zero rows is two different facts. Telling them apart costs a
            // second read, spent only here — on a path nobody is waiting on.
            return self.diagnose_missing(&mut transaction, tenant_id).await;
        };

        let meta_provider: Option<String> = row.try_get(0).map_err(query(CONTEXT_ACTIVATE))?;
        let meta_has_key: Option<bool> = row.try_get(1).map_err(query(CONTEXT_ACTIVATE))?;
        if SecretKind::of(meta_provider.as_deref(), meta_has_key) != SecretKind::ProviderKey {
            return Ok(Activation::NotAProviderKey);
        }

        let workspace = self
            .primary_workspace(tenant_id)
            .await?
            .ok_or_else(provider_no_workspace)?;
        let opened = open_envelope(&row, &workspace, secret_ref, self.vault_key())?;
        let vetted = match managed::vet(opened.expose()) {
            Ok(vetted) => vetted,
            Err(managed::Refused::Malformed) => return Ok(Activation::Malformed),
            Err(managed::Refused::Endpoint(rejection)) => {
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

/// One row's envelope, rebuilt from its columns and opened.
///
/// Columns are read POSITIONALLY, because the order is the contract the
/// statement shares with [`Envelope::from_parts`] — see
/// [`crate::vault`], which reads the same block at its own offset.
fn open_envelope(
    row: &sqlx::postgres::PgRow,
    workspace: &Uuid7,
    name: &str,
    kek: &afd_crypto::secret::Kek,
) -> Result<SecretBytes> {
    let column = |index: usize| {
        row.try_get::<Vec<u8>, _>(index)
            .map_err(query(CONTEXT_ACTIVATE))
    };
    Envelope::from_parts(
        column(ENVELOPE_AT)?,
        &column(ENVELOPE_AT + 1)?,
        &column(ENVELOPE_AT + 2)?,
        &column(ENVELOPE_AT + 3)?,
        column(ENVELOPE_AT + 4)?,
        &column(ENVELOPE_AT + 5)?,
        row.try_get(ENVELOPE_AT + 6)
            .map_err(query(CONTEXT_ACTIVATE))?,
    )
    .map_err(vault_open)?
    .open(kek, &Aad::new(workspace.as_str(), name))
    .map_err(vault_open)
}

impl Providers {
    /// Whether zero rows meant no workspace or no credential.
    async fn diagnose_missing(
        &self,
        transaction: &mut Transaction<'_, sqlx::Postgres>,
        tenant_id: &Uuid7,
    ) -> Result<Activation> {
        let held: Option<String> = sqlx::query_scalar(sql::SELECT_PRIMARY_WORKSPACE)
            .bind(tenant_id.as_str())
            .fetch_optional(&mut **transaction)
            .await
            .map_err(query(CONTEXT_ACTIVATE))?;

        Ok(if held.is_some() {
            Activation::CredentialMissing
        } else {
            Activation::NoWorkspace
        })
    }

    /// The registry entry and the gated selection write, then commit.
    async fn write_activation(
        &self,
        mut transaction: Transaction<'_, sqlx::Postgres>,
        tenant_id: &Uuid7,
        secret_ref: &str,
        provider: &str,
        model: &str,
        now: UnixMillis,
    ) -> Result<Activation> {
        sqlx::query(sql::INSERT_MODEL_ENTRY_IF_ABSENT)
            .bind(self.mint_entry_id(now)?.as_str())
            .bind(tenant_id.as_str())
            .bind(model)
            .bind(secret_ref)
            .bind(now.as_millis())
            .execute(&mut *transaction)
            .await
            .map_err(query(CONTEXT_ACTIVATE))?;

        let stored = sqlx::query(sql::ACTIVATE_SELF_MANAGED)
            .bind(tenant_id.as_str())
            .bind(Posture::SelfManaged.as_str())
            .bind(provider)
            .bind(model)
            .bind(secret_ref)
            .bind(now.as_millis())
            .bind(provider == OPENAI_COMPATIBLE)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(query(CONTEXT_ACTIVATE))?;

        // No row means the gate refused: the catalogue does not carry this
        // (provider, model). The transaction is dropped without a commit, so
        // the registry entry above is rolled back with it.
        let Some(written) = stored else {
            return Ok(Activation::ModelUnknown);
        };
        let selection = crate::provider::selection::read_selection(&written)?;
        transaction
            .commit()
            .await
            .map_err(query(CONTEXT_ACTIVATE))?;

        Ok(Activation::Applied(selection))
    }

    /// A fresh identifier for a registry entry.
    fn mint_entry_id(&self, now: UnixMillis) -> Result<Uuid7> {
        let mut bytes = [0u8; ENTROPY_LEN];
        self.entropy().fill(&mut bytes).map_err(entropy_drained)?;
        Uuid7::encode(now, bytes).map_err(mint_failed)
    }
}

#[cfg(test)]
mod tests {
    use super::effective_model;

    /// What a client sends when it is changing model as well as credential.
    const OVERRIDE: &str = "claude-opus-5";

    /// What an older credential still carries in its body.
    const FROM_CREDENTIAL: &str = "claude-sonnet-5";

    #[test]
    fn the_override_wins_and_the_credential_is_the_fallback() {
        assert_eq!(
            effective_model(Some(OVERRIDE), Some(FROM_CREDENTIAL)),
            Some(OVERRIDE)
        );
        assert_eq!(
            effective_model(None, Some(FROM_CREDENTIAL)),
            Some(FROM_CREDENTIAL)
        );
        assert_eq!(effective_model(Some(OVERRIDE), None), Some(OVERRIDE));
    }

    #[test]
    fn naming_no_model_at_all_is_not_a_model() {
        assert_eq!(effective_model(None, None), None);
    }

    #[test]
    fn a_blank_or_padded_model_is_refused_rather_than_trimmed() {
        // Trimming would store a name the caller did not type and hide the
        // typo; both sources are held to it.
        for refused in [
            "",
            "   ",
            " claude-opus-5",
            "claude-opus-5 ",
            "\tclaude-opus-5",
        ] {
            assert_eq!(effective_model(Some(refused), None), None, "{refused:?}");
            assert_eq!(effective_model(None, Some(refused)), None, "{refused:?}");
        }
    }

    #[test]
    fn a_blank_override_does_not_fall_through_to_the_credential() {
        // The Zig computes `input.model orelse probed.model` and then checks
        // the RESULT, so an empty override is a refusal rather than a reason
        // to use the credential's. Kept: a client that sent a field meant it.
        assert_eq!(effective_model(Some(""), Some(FROM_CREDENTIAL)), None);
    }
}
