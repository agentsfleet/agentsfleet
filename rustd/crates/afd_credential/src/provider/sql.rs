//! Every statement the provider surface runs, and nothing else.
//!
//! Four are resolution's reads; the write half of the selection row joined
//! them when the tenant surface landed. Text is byte-identical to the Zig
//! originals where an original exists.
//!
//! Text is byte-identical to the Zig originals, which live in
//! `state/tenant_provider_resolver.zig` (the selection row and the platform
//! default, written inline at their call sites), `state/secret_probe.zig` (the
//! tenant→workspace bridge, also inline) and `secrets/sql.zig` (the envelope
//! read, which is the one that was already collected upstream).
//!
//! Collected here for the reason [`super`] gives: REVIEW reading these side by
//! side against the Zig is the ONLY enforcement of row-equivalence, and three
//! of the four cannot be read that way while they sit in a function body.

/// The tenant's own provider selection, or nothing.
///
/// Absence is not a failure and not a default row — it is the tenant who has
/// never configured a provider, and it resolves to the platform default the
/// same way an explicit `platform` row does. That collapse is in
/// [`crate::provider`], not here.
///
/// `secret_ref` is NULL under platform mode and carries the vault key name
/// under self-managed; the type system takes that split over from the column in
/// [`Selection`](crate::provider::Selection).
///
/// `$1` tenant.
pub const SELECT_TENANT_MODEL_SELECTION: &str = "\
SELECT mode, provider, model, context_cap_tokens, secret_ref
FROM core.tenant_model_selection
WHERE tenant_id = $1::uuid";

/// The active platform default: provider, key location, model, endpoint, cap.
///
/// `PUT /v1/admin/platform-keys` enforces exactly one active row. The ORDER BY
/// is a determinism guard rather than a selector — a parallel integration test
/// seeding its own active row must not make the choice depend on scan order,
/// and `provider` is the primary key, so the tie-break is a total order that
/// cannot itself tie.
///
/// Read LIVE on every resolution, never from the tenant's own snapshot. That is
/// what makes an admin repointing the default take effect on the next lease for
/// every platform-mode tenant, with no redeploy and no per-tenant write.
pub const SELECT_ACTIVE_PLATFORM_DEFAULT: &str = "\
SELECT provider, source_workspace_id::text, model, base_url, context_cap_tokens
FROM core.platform_provider_defaults
WHERE active = true
ORDER BY updated_at DESC, provider DESC
LIMIT 1";

/// The workspace a tenant's self-managed credentials are held in.
///
/// The earliest-named workspace, which is the same bridge
/// `signup_bootstrap_store` uses for OIDC re-bootstrap. A multi-workspace
/// tenant points its credentials at the first signup-time workspace; pinning a
/// different one would need a `vault_workspace_id` column that does not exist.
///
/// `$1` tenant.
pub const SELECT_PRIMARY_WORKSPACE: &str = "\
SELECT id::text
FROM core.workspaces
WHERE tenant_id = $1::uuid
ORDER BY created_at ASC, id ASC
LIMIT 1";

/// The stored spellings of `core.tenant_model_selection.mode`.
///
/// One declaration each (RULE UFS). These are the same two words
/// [`Posture`](afd_billing::Posture) round-trips through, and they are
/// declared in [`afd_billing::sql::posture`] rather than restated here — the
/// column and the ledger's `posture` column hold the same vocabulary, and two
/// spellings would mean a run billed under one word and selected under another.
pub use afd_billing::sql::posture;

/// Writes the tenant's selection, last-write-wins on its single row.
///
/// The write half of [`SELECT_TENANT_MODEL_SELECTION`], which until now had
/// only a reader: resolution reads this row on every lease, and the tenant's
/// own Models page is what puts it there.
///
/// `created_at` is preserved on conflict and `updated_at` is not, so the read
/// can answer "configured since" rather than "last touched". `EXCLUDED`
/// carries the incoming row, so the preserved value is the stored one by
/// omission rather than by a second read.
///
/// `$1` tenant · `$2` mode · `$3` provider · `$4` model ·
/// `$5` context cap · `$6` secret ref, NULL under the platform posture ·
/// `$7` now.
pub const UPSERT_TENANT_MODEL_SELECTION: &str = "\
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
