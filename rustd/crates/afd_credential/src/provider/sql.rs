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
// that treaty and the delete is the destroyer. Both take the credential lock
// FIRST, and THAT is the serialization point — not the later writes, whose
// lock behaviour differs by `ON CONFLICT` arm and is not the guarantee. With
// the credential row held, the remaining tables are reached in the treaty's
// order and no second participant is inside them.
//
// The treaty exists only because `secret_ref` is TEXT rather than a foreign
// key to `vault.secrets(id)`, so the database cannot refuse an orphaning
// delete on its own. `docs/architecture/tenant_provider_v2.md` §V2-1 is the
// schema change that deletes this machinery; until it lands, this comment and
// the one at the delete are the whole contract.

/// The bridge-and-lock both credential locks share.
///
/// A macro expanding to a LITERAL rather than a `const`, because `concat!`
/// takes literals only and two hand-kept copies of the workspace bridge would
/// drift the moment the primary-workspace rule changed — leaving one verb
/// locking a row in a workspace the other would not have reached (RULE UFS).
///
/// `$1` tenant · `$2` key name.
macro_rules! bridge_and_lock {
    () => {
        "\nFROM vault.secrets s
JOIN (SELECT id FROM core.workspaces
       WHERE tenant_id = $1::uuid
       ORDER BY created_at ASC, id ASC
       LIMIT 1) w ON s.workspace_id = w.id
WHERE s.key_name = $2
FOR UPDATE OF s"
    };
}

/// The credential's shape and envelope, locked, in one statement.
///
/// Four jobs that were four round trips in `tenant_provider.zig`: bridge the
/// tenant to its primary workspace, take the reference lock, read the metadata
/// the write ladder's two credential rungs are decided from, and hand back the
/// envelope columns to open. The join is the same earliest-named-workspace
/// bridge `signup_bootstrap_store` uses.
///
/// `FOR UPDATE OF s` locks the returned `vault.secrets` row and NOTHING else:
/// not `core.workspaces`, and — when the workspace subquery yields no row —
/// nothing at all, since the join then returns no row to lock. Both are
/// wanted. A concurrent workspace rename must not block an activation, and a
/// workspace cannot be deleted from under one: the credential's foreign key
/// cascades from it, so the credential row goes first and this lock covers it.
/// The empty-join case is no gap either — there is no reference to produce,
/// and the caller refuses having written nothing.
///
/// The envelope block keeps the exact column order
/// [`crate::vault::sql::SELECT_SECRET`] uses, because that order is
/// [`afd_crypto::envelope::Envelope::from_parts`]' parameter list. The two
/// metadata columns lead, so the block starts at a fixed offset, and the
/// workspace id is APPENDED after it rather than inserted, so that offset
/// cannot shift.
///
/// That trailing `w.id` is the join's own bridge row, projected rather than
/// re-read. The envelope is opened under the workspace as associated data, and
/// resolving it with a second statement meant a second POOL CONNECTION taken
/// while this transaction already held one and the `FOR UPDATE` row lock —
/// every concurrent activation holding one connection and waiting for another,
/// which is how a bounded pool starves. The subquery here is
/// `SELECT_PRIMARY_WORKSPACE`'s predicate and ordering exactly, so the row is
/// the same one by construction.
///
/// Zero rows means the tenant has no workspace OR holds no such credential —
/// two different refusals, told apart by a second read on the MISS path only,
/// where the extra round trip costs nothing anyone is waiting on.
///
/// `$1` tenant · `$2` key name.
pub const LOCK_CREDENTIAL_FOR_ACTIVATION: &str = concat!(
    "SELECT s.meta_provider, s.meta_has_key,
       s.encrypted_dek, s.dek_nonce, s.dek_tag, s.nonce, s.ciphertext, s.tag, s.kek_version,
       w.id::text",
    bridge_and_lock!()
);

/// The same lock, for a verb that reads nothing off the row.
///
/// Adding a registry entry is a reference PRODUCER exactly as activation is,
/// and takes the identical serialization point — the credential's row lock,
/// first — for the identical reason. What it does NOT need is the metadata or
/// the envelope: it stores a reference and never opens one, so it projects a
/// constant and the row never leaves Postgres.
///
/// Zero rows means the tenant has no workspace OR holds no such credential,
/// told apart by a second read on the miss path only.
///
/// `$1` tenant · `$2` key name.
pub const LOCK_CREDENTIAL_FOR_REFERENCE: &str = concat!("SELECT 1", bridge_and_lock!());

/// The registry entry an activation guarantees exists for its pair.
///
/// The invariant `tenant_model_entries.zig::ensureEntry` enforces, kept where
/// every activation passes: the active `(model, secret_ref)` pair always has a
/// matching row, so the registry's list stays a pure read rather than
/// synthesising one. `DO NOTHING` because re-activating an unchanged pair is
/// not an error and must not bump anything.
///
/// The table carries TWO unique indexes — the `id` primary key and the
/// `(tenant_id, model_id, secret_ref)` domain key — and `ON CONFLICT` across
/// several unique indexes is where Postgres's unprincipled deadlocks live.
/// This is safe from that class for a reason worth stating rather than
/// leaving to luck: `id` is a freshly minted uuidv7 on every call, so the
/// primary key can never be the arbiter that conflicts. One index is ever in
/// play, and the hazard needs two.
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
/// `GREATEST(…, 0)` is the catalogue's own clamp, and it is load-bearing rather
/// than defensive. `core.model_library.context_cap_tokens` is `INTEGER NOT
/// NULL` with NO nonnegative constraint — RULE STS keeps bounds in the
/// application, not in a SQL `CHECK` — so a negative ceiling is a row the
/// schema permits. `model_rate_cache.zig` clamps it with `@max(cap, 0)` at
/// every read, and without the same clamp here a `-1` catalogue row would be
/// STORED as `-1` by this daemon and as `0` by the Zig one: a divergence in
/// the rows themselves, which the state-handoff lane compares.
///
/// It cannot be delegated to [`super::cap`] the way the reset's write is:
/// that ceiling arrives as a `u32` the caller already narrowed, while this one
/// is computed inside the statement and never passes through Rust at all.
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
       GREATEST(COALESCE((SELECT MIN(context_cap_tokens)::int
                            FROM core.model_library
                           WHERE model_id = $4 AND ($7 OR provider = $3)), 0), 0),
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
