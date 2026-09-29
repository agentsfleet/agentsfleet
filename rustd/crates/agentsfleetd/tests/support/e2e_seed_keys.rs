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

use afd_core::clock::UnixMillis;
use afd_crypto::aad::Aad;
use afd_crypto::envelope::Sealer;
use afd_crypto::secret::Kek;
use agentsfleetd::serve::Booted;

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

/// The credential body the platform strategy reads.
///
/// One field. `Platform::interpret` takes the endpoint, model and cap from the
/// defaults ROW and only the key from the vault, so this is the whole shape —
/// and it must be a JSON OBJECT, because `afd_core::json::object_from_slice`
/// refuses an array at the top on purpose.
const PROVIDER_KEY_BODY: &str = r#"{"api_key":"sk-fixture-not-a-credential"}"#;

/// Seals a provider key into the default's source workspace.
///
/// The final precondition the pull path needs. `Providers::resolve` opens
/// `(source_workspace_id, provider)` out of the vault and refuses with
/// "the tenant's provider selection names a vault row that is not held" when it
/// is absent — so a scenario that seeded the DEFAULT without its key resolves
/// an operator gap rather than a runnable fleet.
///
/// Sealed rather than inserted as plaintext: `afd_fleet::Vault` is read-only in
/// this crate (writes are the tenant plane's, M178), and the envelope's
/// additional authenticated data binds the row to `(workspace_id, key_name)` —
/// a fixture that wrote the ciphertext columns by hand would decrypt to a tag
/// failure and look like a corrupt vault. The KEK is the one the daemon booted
/// under, which is why [`GOOD_KEK`] is a constant both halves read.
pub(crate) async fn seed_provider_key(booted: &Booted, workspace: &str, now: UnixMillis) {
    let at = now.as_millis();
    let kek = Kek::from_hex(GOOD_KEK).expect("the lane key is well formed");
    let envelope = Sealer::new()
        .seal(
            &kek,
            &Aad::new(workspace, PROVIDER),
            PROVIDER_KEY_BODY.as_bytes(),
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
            encrypted_dek, dek_nonce, dek_tag, nonce, ciphertext, tag,
            created_at, updated_at)
         VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7, $8, $9, $10, $11, $11)
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
    .bind(at)
    .execute(&mut *connection)
    .await
    .expect("the provider key seed must run");
}

/// The key name a tenant-plane scenario authenticates under.
///
/// One name per tenant is enough: `uq_api_keys_tenant_id_key_name` is scoped
/// to the tenant, and every scenario mints its own tenant id.
const TENANT_KEY_NAME: &str = "e2e-tenant-plane";

/// Stores the digest of `token` as an active API key for `tenant`.
///
/// The digest and not the token, exactly as the production minting writes it:
/// the daemon's directory authenticates by digest lookup, so a seeded PLAIN
/// token would prove a path nothing ships. The token itself never touches the
/// database — the caller presents it over HTTP and the daemon re-digests it.
pub(crate) async fn seed_tenant_key(booted: &Booted, tenant: &str, token: &str, now: UnixMillis) {
    let presented =
        afd_auth::credential::Presented::new(token).expect("the fixture token is well formed");
    let digest = afd_auth::directory::Digest::of(&presented);
    let mut connection = booted
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO core.api_keys
           (id, tenant_id, key_name, description, key_hash, created_by, active,
            revoked_at, created_at, updated_at)
         VALUES ($1::uuid, $2::uuid, $3, '', $4, 'e2e', TRUE, NULL, $5, $5)",
    )
    .bind(afd_db::test_util::mint_id())
    .bind(tenant)
    .bind(TENANT_KEY_NAME)
    .bind(digest.as_str())
    .bind(now.as_millis())
    .execute(&mut *connection)
    .await
    .expect("the tenant credential seed must run");
}

/// Seals a credential the ACTIVATION ladder admits, under `name`.
///
/// [`seed_provider_key`]'s body deliberately carries no `provider` field — the
/// runner path decrypts it only to dial, and a body that would also activate
/// would let a scenario pass the ladder by accident. The tenant-plane walk
/// needs the opposite: a body naming its provider (the field `UZ-PROVIDER-003`
/// refuses without) and the projection columns the metadata gate reads before
/// any decrypt, sealed under a name of the caller's own so the runner fixture
/// keeps its shape.
pub(crate) async fn seed_activatable_key(booted: &Booted, workspace: &str, name: &str) {
    let kek = Kek::from_hex(GOOD_KEK).expect("the lane key is well formed");
    let body = format!(r#"{{"provider":"{PROVIDER}","api_key":"sk-fixture-not-a-credential"}}"#);
    let envelope = Sealer::new()
        .seal(&kek, &Aad::new(workspace, name), body.as_bytes())
        .expect("the walk credential seals");

    let mut connection = booted
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO vault.secrets
           (id, workspace_id, key_name, kek_version,
            encrypted_dek, dek_nonce, dek_tag, nonce, ciphertext, tag,
            created_at, updated_at,
            meta_kind, meta_provider, meta_has_key)
         VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7, $8, $9, $10, $11, $11,
                 'provider_key', $12, TRUE)",
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
    .bind(afd_core::clock::now().as_millis())
    .bind(PROVIDER)
    .execute(&mut *connection)
    .await
    .expect("the walk credential seed must run");
}
