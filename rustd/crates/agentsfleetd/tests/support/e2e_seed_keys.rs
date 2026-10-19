//! The sealed credentials a scenario seeds: the provider key the pull path
//! decrypts to dial, the tenant API key the tenant plane authenticates by, and
//! a key the activation ladder admits.
//!
//! Split from `e2e_seed.rs` by concern (RULE FLL): the rows there are the pull
//! path's preconditions and move when it starts consulting something new;
//! these move when the vault's envelope or the key tables do.
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use afd_crypto::aad::Aad;
use afd_crypto::envelope::Sealer;
use afd_crypto::secret::Kek;
use agentsfleetd::serve::Booted;
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;

use crate::e2e::{GOOD_KEK, PROVIDER};

/// The vault row a seeded provider key is written under.
///
/// MINTED, where `e2e_seed.rs`'s catalogue row is a constant, and the difference is
/// the rule: a catalogue rate is one shared row every scenario writes the same
/// way, but a provider key belongs to ONE workspace and `scenario` mints a fresh
/// one per run. So the `ON CONFLICT (workspace_id, key_name)` arm never fires
/// between two scenarios, the PRIMARY KEY is what they would collide on, and a
/// shared constant would drop the second scenario's key and leave it resolving
/// against a workspace that has none. `mint_id` shapes it so
/// `ck_vault_secrets_id_uuidv7` passes.
fn vault_row() -> String {
    afd_db::test_util::mint_id()
}

/// The bearer key every sealed fixture credential carries.
const FIXTURE_API_KEY: &str = "sk-fixture-not-a-credential";

/// What the tenant plane's write stores beside a provider key, so reads can
/// rule on it before any decrypt.
const PROVIDER_KEY_KIND: &str = "provider_key";

/// The credential body runtime resolution reads: it vets the same body
/// activation does, so the body names its provider.
fn provider_key_body() -> String {
    format!(r#"{{"provider":"{PROVIDER}","api_key":"{FIXTURE_API_KEY}"}}"#)
}

/// Seals a provider key into the default's source workspace.
///
/// The final precondition the pull path needs. Runtime resolution opens
/// `(source_workspace_id, provider)` out of the vault only when the row's
/// projection names that provider and holds a key, and it vets the body
/// before dialling, so the seed writes both halves the tenant plane's write
/// would. A scenario that seeded the DEFAULT without them resolves an operator
/// gap rather than a runnable fleet.
///
/// Sealed rather than inserted as plaintext: `afd_fleet::Vault` is read-only in
/// this crate (writes are the tenant plane's, M178), and the envelope's
/// additional authenticated data binds the row to `(workspace_id, key_name)` —
/// a fixture that wrote the ciphertext columns by hand would decrypt to a tag
/// failure and look like a corrupt vault. The KEK is the one the daemon booted
/// under, which is why [`GOOD_KEK`] is a constant both halves read.
pub(crate) async fn seed_provider_key(booted: &Booted, workspace: &str, now: Timestamp) {
    let kek = Kek::from_hex(GOOD_KEK).expect("the lane key is well formed");
    let envelope = Sealer::new()
        .seal(
            &kek,
            &Aad::new(workspace, PROVIDER),
            provider_key_body().as_bytes(),
        )
        .expect("the fixture credential seals");

    let mut connection = booted
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO vault.secrets
           (id, workspace_id, key_name, kek_version,
            dek_ciphertext, dek_nonce, dek_tag, secret_nonce, secret_ciphertext, secret_tag,
            created_at, updated_at,
            meta_kind, meta_provider, meta_has_key)
         VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7, $8, $9, $10, $11, $11,
                 $12, $3, TRUE)
         ON CONFLICT (workspace_id, key_name) DO NOTHING",
    )
    .bind(vault_row())
    .bind(workspace)
    .bind(PROVIDER)
    .bind(envelope.kek_version())
    .bind(envelope.wrapped_dek())
    .bind(envelope.dek_nonce().as_slice())
    .bind(envelope.dek_tag().as_slice())
    .bind(envelope.payload_nonce().as_slice())
    .bind(envelope.payload_ciphertext())
    .bind(envelope.payload_tag().as_slice())
    .bind(now.to_sqlx())
    .bind(PROVIDER_KEY_KIND)
    .execute(&mut *connection)
    .await
    .expect("the provider key seed must run");
}

/// The key name a tenant-plane scenario authenticates under.
///
/// One name per tenant is enough: `uq_api_keys_tenant_id_name` is scoped
/// to the tenant, and every scenario mints its own tenant id.
const TENANT_KEY_NAME: &str = "e2e-tenant-plane";

/// Stores the digest of `token` as an active API key for `tenant`.
///
/// The digest and not the token, exactly as the production minting writes it:
/// the daemon's directory authenticates by digest lookup, so a seeded PLAIN
/// token would prove a path nothing ships. The token itself never touches the
/// database — the caller presents it over HTTP and the daemon re-digests it.
pub(crate) async fn seed_tenant_key(booted: &Booted, tenant: &str, token: &str, now: Timestamp) {
    let presented =
        afd_auth::credential::Presented::new(token).expect("the fixture token is well formed");
    let digest = afd_auth::directory::Digest::of(&presented);
    let mut connection = booted
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO identity.api_keys
           (id, tenant_id, name, description, key_hash, created_by_subject,
            revoked_at, created_at, updated_at)
         VALUES ($1::uuid, $2::uuid, $3, '', $4, 'e2e', NULL, $5, $5)",
    )
    .bind(afd_db::test_util::mint_id())
    .bind(tenant)
    .bind(TENANT_KEY_NAME)
    .bind(digest.as_str())
    .bind(now.to_sqlx())
    .execute(&mut *connection)
    .await
    .expect("the tenant credential seed must run");
}

/// Seals a credential the ACTIVATION ladder admits, under `name`: the same
/// body and projection [`seed_provider_key`] writes, under a name of the
/// caller's own.
pub(crate) async fn seed_activatable_key(booted: &Booted, workspace: &str, name: &str) {
    seal_named(booted, workspace, name, Some(PROVIDER_KEY_KIND)).await;
}

/// Seals the same body under `name` with NO projection, as a row written
/// before the vault recorded what it holds: the metadata gate refuses it
/// before any decrypt (`UZ-PROVIDER-003`).
pub(crate) async fn seed_unprojected_key(booted: &Booted, workspace: &str, name: &str) {
    seal_named(booted, workspace, name, None).await;
}

async fn seal_named(booted: &Booted, workspace: &str, name: &str, kind: Option<&str>) {
    let kek = Kek::from_hex(GOOD_KEK).expect("the lane key is well formed");
    let envelope = Sealer::new()
        .seal(
            &kek,
            &Aad::new(workspace, name),
            provider_key_body().as_bytes(),
        )
        .expect("the walk credential seals");
    let projected = kind.map(|_kind| PROVIDER);

    let mut connection = booted
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO vault.secrets
           (id, workspace_id, key_name, kek_version,
            dek_ciphertext, dek_nonce, dek_tag, secret_nonce, secret_ciphertext, secret_tag,
            created_at, updated_at,
            meta_kind, meta_provider, meta_has_key)
         VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7, $8, $9, $10, $11, $11,
                 $12, $13, $14)",
    )
    .bind(vault_row())
    .bind(workspace)
    .bind(name)
    .bind(envelope.kek_version())
    .bind(envelope.wrapped_dek())
    .bind(envelope.dek_nonce().as_slice())
    .bind(envelope.dek_tag().as_slice())
    .bind(envelope.payload_nonce().as_slice())
    .bind(envelope.payload_ciphertext())
    .bind(envelope.payload_tag().as_slice())
    .bind(afd_core::clock::now().to_sqlx())
    .bind(kind)
    .bind(projected)
    .bind(kind.map(|_kind| true))
    .execute(&mut *connection)
    .await
    .expect("the walk credential seed must run");
}
