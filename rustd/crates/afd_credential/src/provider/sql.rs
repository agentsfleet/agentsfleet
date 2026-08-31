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

// ── Activation: the write ladder's statements ───────────────────────────────
//
// The lock order these participate in is the one `afd_vault`'s delete path
// spells at `afd_vault/src/sql.rs`: the credential row, then the entries that
// name it, then the tenant's selection. Activation is the PRODUCER side of
// that treaty and the delete is the destroyer; both take the credential lock
// FIRST, which is the serialization point, and reach the other two tables by
// WRITING them — a write is a lock, in the same order.
//
// The treaty exists only because `secret_ref` is TEXT rather than a foreign
// key to `vault.secrets(id)`, so the database cannot refuse an orphaning
// delete on its own. `docs/architecture/tenant_provider_v2.md` §V2-1 is the
// schema change that deletes this machinery; until it lands, this comment and
// the one at the delete are the whole contract.

/// The credential's shape and envelope, locked, in one statement.
///
/// Four jobs that were four round trips in `tenant_provider.zig`: bridge the
/// tenant to its primary workspace, take the reference lock, read the metadata
/// the write ladder's two credential rungs are decided from, and hand back the
/// envelope columns to open. The join is the same earliest-named-workspace
/// bridge `signup_bootstrap_store` uses.
///
/// `FOR UPDATE OF s` locks the credential and NOT the workspace row: a
/// concurrent workspace rename must not block an activation, and a workspace
/// cannot be deleted out from under one — the credential's own foreign key
/// cascades from it, so the credential row is gone first and the lock covers
/// it.
///
/// The envelope block keeps the exact column order
/// [`crate::vault::sql::SELECT_SECRET`] uses, because that order is
/// [`afd_crypto::envelope::Envelope::from_parts`]' parameter list. The two
/// metadata columns lead, so the block starts at a fixed offset.
///
/// Zero rows means the tenant has no workspace OR holds no such credential —
/// two different refusals, told apart by a second read on the MISS path only,
/// where the extra round trip costs nothing anyone is waiting on.
///
/// `$1` tenant · `$2` key name.
pub const LOCK_CREDENTIAL_FOR_ACTIVATION: &str = "\
SELECT s.meta_provider, s.meta_has_key,
       s.encrypted_dek, s.dek_nonce, s.dek_tag, s.nonce, s.ciphertext, s.tag, s.kek_version
FROM vault.secrets s
JOIN (SELECT id FROM core.workspaces
       WHERE tenant_id = $1::uuid
       ORDER BY created_at ASC, id ASC
       LIMIT 1) w ON s.workspace_id = w.id
WHERE s.key_name = $2
FOR UPDATE OF s";

/// The registry entry an activation guarantees exists for its pair.
///
/// The invariant `tenant_model_entries.zig::ensureEntry` enforces, kept where
/// every activation passes: the active `(model, secret_ref)` pair always has a
/// matching row, so the registry's list stays a pure read rather than
/// synthesising one. `DO NOTHING` because re-activating an unchanged pair is
/// not an error and must not bump anything.
///
/// `$1` id · `$2` tenant · `$3` model · `$4` secret ref · `$5` now.
pub const INSERT_MODEL_ENTRY_IF_ABSENT: &str = "\
INSERT INTO core.tenant_model_entries
    (id, tenant_id, model_id, secret_ref, created_at, updated_at)
VALUES ($1::uuid, $2::uuid, $3, $4, $5, $5)
ON CONFLICT (tenant_id, model_id, secret_ref) DO NOTHING";

/// Activates a self-managed selection, gated on the catalogue in one snapshot.
///
/// The gate and the write are ONE statement, so nothing can delete the model
/// between checking it and storing its ceiling. `rows_affected() == 0` IS the
/// "not catalogued" refusal; a separate `SELECT` first would be a race that
/// stores a ceiling for a model that no longer exists.
///
/// `$7` carries whether the credential's provider is the compatible one, and
/// it is what makes this one statement rather than two near-identical ones:
///
/// - A NAMED provider must be catalogued for its own provider. The `WHERE`
///   requires that exact row, so a miss writes nothing, and the ceiling is
///   that row's own — `MIN` over a single row is that row.
/// - The COMPATIBLE provider hosts a user's own endpoint, so the model is
///   absent from the platform catalogue by design. The `WHERE` passes
///   unconditionally, and the ceiling is the smallest any provider publishes
///   for that model — a context window is a property of the MODEL, not of the
///   host serving it. `COALESCE` supplies the unknown/auto sentinel when the
///   catalogue knows the model under no provider at all.
///
/// `RETURNING` is what lets the response echo the write instead of re-reading
/// it, which is stricter: a re-read can observe a racing writer's row.
///
/// `$1` tenant · `$2` mode · `$3` provider · `$4` model · `$5` secret ref ·
/// `$6` now · `$7` whether the provider is the compatible one.
pub const ACTIVATE_SELF_MANAGED: &str = "\
INSERT INTO core.tenant_model_selection
    (tenant_id, mode, provider, model, context_cap_tokens, secret_ref, created_at, updated_at)
SELECT $1::uuid, $2, $3, $4,
       COALESCE((SELECT MIN(context_cap_tokens)::int
                   FROM core.model_library
                  WHERE model_id = $4 AND ($7 OR provider = $3)), 0),
       $5, $6, $6
 WHERE $7 OR EXISTS (SELECT 1 FROM core.model_library
                      WHERE provider = $3 AND model_id = $4)
ON CONFLICT (tenant_id) DO UPDATE SET
    mode = EXCLUDED.mode,
    provider = EXCLUDED.provider,
    model = EXCLUDED.model,
    context_cap_tokens = EXCLUDED.context_cap_tokens,
    secret_ref = EXCLUDED.secret_ref,
    updated_at = EXCLUDED.updated_at
RETURNING mode, provider, model, context_cap_tokens, secret_ref";
