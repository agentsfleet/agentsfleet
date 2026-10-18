//! The platform rows a drained lease resolves its provider through, and the
//! release that removes them.
//!
//! A platform-posture tenant — every tenant the drain seeds — resolves its
//! provider through the one active `model.platform_defaults` row, the
//! catalogue row it names, and the credential sealed in its source workspace.
//! So the drain stages all three, under a key minted for the run: the priced
//! row and the default through `afd_db`'s shared seeds, the credential through
//! the vault's own write path.
//!
//! # The only rows the prefix sweep cannot find
//!
//! A platform default is deployment-wide and keyed by provider, and it holds a
//! foreign key on its source workspace that would refuse the sweep's delete.
//! So [`release`] removes the default and the catalogue row this run wrote
//! before the prefix sweep runs. The sealed credential sits in a drain
//! workspace and leaves with it.

use std::sync::Arc;

use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_crypto::secret::Kek;
use afd_db::test_util::{DefaultSeed, PlatformModelSeed, Prices};
use afd_vault::{SecretBody, SecretName, Vault};
use serde_json::value::RawValue;

use super::stage::INDEX_BASE;
use crate::datastores::Datastores;
use crate::error::{ErrorKind, Result, seed_refused};
use crate::lane::lease::seed::{self, SEEDED_AT};

/// Identifier kind for the catalogue row, apart from the seed's three.
const KIND_MODEL: u32 = 4;

/// The provider the platform default names. A real one, because the policy
/// build derives the run's egress from it.
const PROVIDER: &str = "anthropic";

/// A model spelling no real catalogue carries, so the row is unmistakably
/// this lane's and releasing it can never remove an operator's.
const MODEL: &str = "bench-drain-model";

/// A credential shaped like a provider key and worth nothing.
const CREDENTIAL_BODY: &str = r#"{"api_key":"bench-not-a-credential"}"#;

/// The context window the catalogue row advertises; the column is required.
const CONTEXT_CAP_TOKENS: i32 = 200_000;

/// Catalogue prices, Anthropic-shaped so the estimate floor prices above zero.
const PRICES: Prices = Prices {
    input_nanos_per_mtok: 3_000_000_000,
    cached_input_nanos_per_mtok: 300_000_000,
    output_nanos_per_mtok: 15_000_000_000,
};

/// A key minted for this run alone, so nothing it seals opens anywhere else.
///
/// # Errors
///
/// `CredentialUnsealable` when the host will not supply entropy.
pub(super) fn minted_kek() -> Result<Kek> {
    let mut bytes = [0_u8; afd_crypto::KEY_LEN];
    Entropy::new().fill(&mut bytes)?;
    Ok(Kek::from_bytes(bytes))
}

/// The catalogue row, the credential sealed under `kek`, and the default
/// naming both.
///
/// # Errors
///
/// Whatever Postgres refused, a credential the vault would not store, and
/// `PlatformDefaultHeld` when another run holds the default.
pub(super) async fn stage(stores: &Datastores, kek: &Arc<Kek>) -> Result<()> {
    let workspace = source_workspace();
    let mut connection = stores.database.acquire().await?;
    PlatformModelSeed::new(&model_row(), PROVIDER, MODEL, CONTEXT_CAP_TOKENS, PRICES)
        .at(SEEDED_AT)
        .insert(&mut *connection)
        .await
        .map_err(seed_refused)?;
    drop(connection);
    seal_credential(stores, kek, &workspace).await?;
    let mut connection = stores.database.acquire().await?;
    let claimed = DefaultSeed {
        provider: PROVIDER,
        source_workspace_id: &workspace,
        model: MODEL,
        context_cap_tokens: CONTEXT_CAP_TOKENS,
        created_at: SEEDED_AT,
    }
    .claim(&mut *connection)
    .await
    .map_err(seed_refused)?;
    if !claimed {
        return Err(ErrorKind::PlatformDefaultHeld { provider: PROVIDER }.into());
    }
    Ok(())
}

/// Seal the credential into `workspace`'s vault under the name resolution
/// opens it by — the provider's.
async fn seal_credential(stores: &Datastores, kek: &Arc<Kek>, workspace: &str) -> Result<()> {
    let body = RawValue::from_string(CREDENTIAL_BODY.to_owned())
        .map_err(|source| ErrorKind::CredentialMalformed { source })?;
    let workspace = Uuid7::parse(workspace)?;
    Vault::new(stores.database.clone(), Arc::clone(kek), Entropy::new())
        .create(
            &workspace,
            &SecretName::parse(PROVIDER)?,
            &SecretBody::parse(&body)?,
            SEEDED_AT,
        )
        .await?;
    Ok(())
}

/// Remove the platform default and the catalogue row this run wrote.
///
/// Scoped to this run's source workspace and row id, so a default another run
/// staged is never the one removed. Safe to call after a staging that failed
/// anywhere, including before either row was written.
///
/// # Errors
///
/// Whatever Postgres refused.
pub(super) async fn release(stores: &Datastores) -> Result<()> {
    let mut connection = stores.database.acquire().await?;
    DefaultSeed::release(&mut *connection, PROVIDER, &source_workspace())
        .await
        .map_err(seed_refused)?;
    PlatformModelSeed::remove(&mut *connection, &model_row())
        .await
        .map_err(seed_refused)?;
    Ok(())
}

/// The workspace the platform credential is sealed into: the first fleet's.
///
/// Derived from the index rather than carried from staging, so a staging that
/// failed halfway can still be released.
fn source_workspace() -> String {
    seed::identities(INDEX_BASE).workspace
}

/// The catalogue row's identifier, derived for the same reason.
fn model_row() -> String {
    seed::identifier(KIND_MODEL, INDEX_BASE)
}
