//! The platform rows a drained lease resolves its provider through, and the
//! release that removes them.
//!
//! A platform-posture tenant — every tenant the drain seeds — resolves its
//! provider through the one active `core.platform_provider_defaults` row, the
//! catalogue row it names, and the credential sealed in its source workspace.
//! So the drain stages all three, under a key minted for the run.
//!
//! # The only rows the prefix sweep cannot find
//!
//! A platform default is deployment-wide and keyed by provider, and it holds a
//! foreign key on its source workspace that would refuse the sweep's delete.
//! So [`release`] removes the default and the catalogue row this run wrote
//! before the prefix sweep runs. The sealed credential sits in a drain
//! workspace and leaves with it.

use afd_crypto::aad::Aad;
use afd_crypto::entropy::Entropy;
use afd_crypto::envelope::Sealer;
use afd_crypto::secret::Kek;

use super::stage::INDEX_BASE;
use crate::datastores::Datastores;
use crate::error::{ErrorKind, Result};
use crate::lane::lease::seed::{self, SEEDED_AT};

/// Identifier kind for the catalogue row, apart from the seed's three.
const KIND_MODEL: u32 = 4;

/// Identifier kind for the sealed platform credential.
const KIND_SECRET: u32 = 5;

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
const INPUT_NANOS_PER_MTOK: i64 = 3_000_000_000;
/// As [`INPUT_NANOS_PER_MTOK`], for cached input.
const CACHED_INPUT_NANOS_PER_MTOK: i64 = 300_000_000;
/// As [`INPUT_NANOS_PER_MTOK`], for output.
const OUTPUT_NANOS_PER_MTOK: i64 = 15_000_000_000;

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
/// Whatever Postgres refused, a credential that would not seal, and
/// `PlatformDefaultHeld` when another run holds the default.
pub(super) async fn stage(stores: &Datastores, kek: &Kek) -> Result<()> {
    let workspace = source_workspace();
    let mut connection = stores.database.acquire().await?;
    sqlx::query(
        "INSERT INTO core.model_library \
           (id, model_id, provider, context_cap_tokens, input_nanos_per_mtok, \
            cached_input_nanos_per_mtok, output_nanos_per_mtok, created_at, updated_at) \
         VALUES ($1::uuid, $2, $3, $4, $5, $6, $7, $8, $8) \
         ON CONFLICT (provider, model_id) DO NOTHING",
    )
    .bind(model_row())
    .bind(MODEL)
    .bind(PROVIDER)
    .bind(CONTEXT_CAP_TOKENS)
    .bind(INPUT_NANOS_PER_MTOK)
    .bind(CACHED_INPUT_NANOS_PER_MTOK)
    .bind(OUTPUT_NANOS_PER_MTOK)
    .bind(SEEDED_AT)
    .execute(&mut *connection)
    .await?;
    seal_credential(&mut connection, kek, &workspace).await?;
    claim_default(&mut connection, &workspace).await
}

/// Seal the credential into `workspace`'s vault under the name resolution
/// opens it by — the provider's.
async fn seal_credential(
    connection: &mut sqlx::PgConnection,
    kek: &Kek,
    workspace: &str,
) -> Result<()> {
    let sealed = Sealer::new().seal(
        kek,
        &Aad::new(workspace, PROVIDER),
        CREDENTIAL_BODY.as_bytes(),
    )?;
    sqlx::query(
        "INSERT INTO vault.secrets \
           (id, workspace_id, key_name, kek_version, encrypted_dek, dek_nonce, \
            dek_tag, nonce, ciphertext, tag, created_at, updated_at) \
         VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7, $8, $9, $10, $11, $11)",
    )
    .bind(seed::identifier(KIND_SECRET, INDEX_BASE))
    .bind(workspace)
    .bind(PROVIDER)
    .bind(sealed.kek_version())
    .bind(sealed.wrapped_dek())
    .bind(sealed.dek_nonce().as_slice())
    .bind(sealed.dek_tag().as_slice())
    .bind(sealed.payload_nonce().as_slice())
    .bind(sealed.payload_ciphertext())
    .bind(sealed.payload_tag().as_slice())
    .bind(SEEDED_AT)
    .execute(connection)
    .await?;
    Ok(())
}

/// Make this run's credential the platform default, refusing to take one
/// another run holds.
async fn claim_default(connection: &mut sqlx::PgConnection, workspace: &str) -> Result<()> {
    let claimed = sqlx::query(
        "INSERT INTO core.platform_provider_defaults \
           (provider, source_workspace_id, active, model, context_cap_tokens, \
            created_at, updated_at) \
         VALUES ($1, $2::uuid, TRUE, $3, $4, $5, $5) \
         ON CONFLICT (provider) DO NOTHING",
    )
    .bind(PROVIDER)
    .bind(workspace)
    .bind(MODEL)
    .bind(CONTEXT_CAP_TOKENS)
    .bind(SEEDED_AT)
    .execute(connection)
    .await?
    .rows_affected();
    if claimed == 0 {
        return Err(ErrorKind::PlatformDefaultHeld { provider: PROVIDER }.into());
    }
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
    sqlx::query(
        "DELETE FROM core.platform_provider_defaults \
         WHERE provider = $1 AND source_workspace_id = $2::uuid",
    )
    .bind(PROVIDER)
    .bind(source_workspace())
    .execute(&mut *connection)
    .await?;
    sqlx::query("DELETE FROM core.model_library WHERE id = $1::uuid")
        .bind(model_row())
        .execute(&mut *connection)
        .await?;
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
