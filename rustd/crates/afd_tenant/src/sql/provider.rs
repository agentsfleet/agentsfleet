//! Statements over the tenant's provider selection.
//!
//! The read half already exists in `afd_credential`, which resolves a key for
//! every lease. What lives here is the surface a TENANT drives: the same row
//! read for display, and the upsert that changes it. They are kept apart
//! because they answer different questions on different paths — one runs on
//! the hot lease path and must stay a single indexed read, the other runs once
//! when somebody clicks Save.

/// The tenant's own view of its selection, for rendering rather than dialling.
///
/// `created_at` rides along because the dashboard distinguishes a tenant that
/// never configured a provider from one that explicitly reset to platform
/// mode, and a row's existence is what tells them apart.
///
/// `$1` tenant.
pub const SELECT_SELECTION: &str = "\
SELECT mode, provider, model, context_cap_tokens, secret_ref, created_at, updated_at
FROM core.tenant_model_selection
WHERE tenant_id = $1::uuid";

/// Writes the selection, last-write-wins on the tenant's single row.
///
/// `created_at` is preserved on conflict and `updated_at` is not, which is
/// what lets the read above answer "configured since" rather than "last
/// touched". `EXCLUDED` carries the incoming row, so the preserved value is
/// the stored one by omission rather than by a second read.
///
/// `$1` tenant · `$2` mode · `$3` provider · `$4` model ·
/// `$5` context cap · `$6` secret ref, NULL under platform mode · `$7` now.
pub const UPSERT_SELECTION: &str = "\
INSERT INTO core.tenant_model_selection
    (tenant_id, mode, provider, model, context_cap_tokens, secret_ref, created_at, updated_at)
VALUES ($1::uuid, $2, $3, $4, $5, $6, $7, $7)
ON CONFLICT (tenant_id) DO UPDATE SET
    mode = EXCLUDED.mode,
    provider = EXCLUDED.provider,
    model = EXCLUDED.model,
    context_cap_tokens = EXCLUDED.context_cap_tokens,
    secret_ref = EXCLUDED.secret_ref,
    updated_at = EXCLUDED.updated_at";

/// The non-secret shape of the credential a selection names.
///
/// The write ladder's second and third rungs in one read, and it opens
/// nothing. `vault.secrets` carries `meta_provider` and `meta_has_key` beside
/// the ciphertext precisely so a caller can ask what KIND of credential a row
/// holds without holding it — no row is rung two, a row whose metadata does not
/// describe a provider key is rung three.
///
/// The Zig path decrypts to answer this. Reading the metadata instead means
/// the refusal path never has a plaintext key in memory at all, which is a
/// smaller surface for the same answer.
///
/// `$1` workspace · `$2` key name.
pub const SELECT_SECRET_SHAPE: &str = "\
SELECT meta_provider, meta_has_key
FROM vault.secrets
WHERE workspace_id = $1::uuid AND key_name = $2";
