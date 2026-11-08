//! Every statement the provider surface runs, and nothing else.
//!
//! Each is a `.sql` file beside this module, schema-qualified, and checked
//! against the rebuilt schema by `afd_db`'s `test_every_sql_file_prepares`.
//! Collected here so REVIEW can read every statement side by side with the
//! schema it touches, which a statement sitting in a function body hides.
//!
//! # A selection is a pointer
//!
//! `model.tenant_selections.tenant_model_id` is NULL for the platform default
//! and otherwise names one of the tenant's own `model.tenant_models` rows,
//! through a composite foreign key that also carries `tenant_id` — so a
//! pointer at another tenant's model is refused by the database itself. The
//! provider a self-managed selection runs on is the CREDENTIAL's
//! (`vault.secrets.meta_provider`), and its context ceiling is the catalogue's,
//! both read live rather than copied onto the selection.
//!
//! # The primary-workspace bridge
//!
//! A tenant's self-managed credentials live in its earliest-named workspace,
//! the same bridge `signup_bootstrap_store` uses for OIDC re-bootstrap. Four
//! statements here and the two entry reads in `registered` and `runtime` spell
//! it, each as a whole file, because a fragment `concat!`ed into several
//! statements is a file the prepare check cannot run.
//! [`SELECT_PRIMARY_WORKSPACE`] is the canonical spelling; the others repeat
//! its predicate and ordering exactly, and every one is prepared against the
//! schema.

/// The tenant's selection, with what it points at, or nothing.
///
/// Absence is not a failure and not a default row — it is the tenant who has
/// never configured a provider, and it resolves to the platform default the
/// same way a row with a NULL pointer does. That collapse is in
/// [`crate::provider`], not here.
///
/// Every join is LEFT, so a selection row always comes back: a NULL pointer
/// reads as the platform posture, and a credential deleted out of band reads
/// as a NULL provider rather than as no selection at all.
///
/// The ceiling is the smallest the catalogue publishes for the model: under
/// its own provider for a named credential, and under ANY provider for the
/// compatible one, whose user-hosted endpoint the catalogue cannot carry. A
/// context window is a property of the MODEL, not of the host serving it.
/// `COALESCE` supplies the unknown/auto sentinel when the catalogue knows the
/// model under no provider at all, and `GREATEST(…, 0)` is the catalogue's own
/// clamp: `context_cap_tokens` is `INTEGER NOT NULL` with no nonnegative
/// constraint, because bounds are the application's (RULE STS).
///
/// `$1` tenant · `$2` the compatible provider's name.
pub const SELECT_TENANT_SELECTION: &str = include_str!("select_tenant_selection.sql");

/// Whether the tenant's selection points at one registry entry.
///
/// No row is a tenant that never chose, and a NULL pointer compares false, so
/// both answer "not the active one".
///
/// `$1` tenant · `$2` entry.
pub const SELECT_SELECTION_HOLDS: &str = include_str!("select_selection_holds.sql");

/// The active platform default: provider, key location, model, endpoint, cap.
///
/// `PUT /v1/admin/platform-keys` enforces exactly one active row. The ORDER BY
/// is a determinism guard rather than a selector — a parallel integration test
/// seeding its own active row must not make the choice depend on scan order,
/// and `provider` is the primary key, so the tie-break is a total order that
/// cannot itself tie.
///
/// The model is joined LEFT so an active row whose catalogue pointer is NULL
/// still comes back, to be refused as corrupt rather than read as absent.
///
/// Read LIVE on every resolution, never from the tenant's own row. That is
/// what makes an admin repointing the default take effect on the next lease for
/// every platform-mode tenant, with no redeploy and no per-tenant write.
pub const SELECT_ACTIVE_PLATFORM_DEFAULT: &str = include_str!("select_active_platform_default.sql");

/// The workspace a tenant's self-managed credentials are held in.
///
/// A multi-workspace tenant points its credentials at the first signup-time
/// workspace; pinning a different one would need a `vault_workspace_id`
/// column that does not exist.
///
/// `$1` tenant.
pub const SELECT_PRIMARY_WORKSPACE: &str = include_str!("select_primary_workspace.sql");

/// Points the tenant back at the platform default: a NULL pointer.
///
/// An explicit row rather than a deleted one, so the dashboard can tell "never
/// configured" from "explicitly reset". `created_at` is preserved on conflict,
/// so the row answers "configured since" rather than "last touched".
///
/// `$1` tenant.
pub const RESET_TENANT_SELECTION: &str = include_str!("reset_tenant_selection.sql");

// ── Activation: the write ladder's statements ───────────────────────────────
//
// The lock order these participate in is the one `afd_vault`'s delete path
// spells at `afd_vault/src/sql/mod.rs`: the credential row, then the entries
// that name it, then the tenant's selection. Activation is the PRODUCER side of
// that protocol and the delete is the destroyer. Both take the credential lock
// FIRST, and THAT is the serialization point — not the later writes, whose
// lock behaviour differs by `ON CONFLICT` arm and is not the guarantee. With
// the credential row held, the remaining tables are reached in that order and
// no second participant is inside them.
//
// The protocol exists only because `model.tenant_models.secret_ref` is TEXT
// naming a `(workspace_id, key_name)` pair rather than a foreign key, so the
// database cannot refuse an orphaning credential delete on its own.

/// The credential's shape and envelope, locked, in one statement.
///
/// Four jobs in one round trip: bridge the tenant to its primary workspace,
/// take the reference lock, read the metadata the write ladder's two
/// credential rungs are decided from, and hand back the envelope columns to
/// open.
///
/// `FOR UPDATE OF s` locks the returned `vault.secrets` row and NOTHING else:
/// not `identity.workspaces`, and — when the workspace subquery yields no row —
/// nothing at all, since the join then returns no row to lock. Both are
/// wanted. A concurrent workspace rename must not block an activation, and a
/// workspace cannot be deleted from under one: the credential's foreign key
/// cascades from it, so the credential row goes first and this lock covers it.
///
/// The bridge row's id is projected as `workspace_id` rather than re-read. The
/// envelope is opened under the workspace as associated data, and resolving it
/// with a second statement meant a second POOL CONNECTION taken while this
/// transaction already held one and the row lock — every concurrent activation
/// holding one connection and waiting for another, which is how a bounded pool
/// starves.
///
/// Zero rows means the tenant has no workspace OR holds no such credential —
/// two different refusals, told apart by a second read on the MISS path only,
/// where the extra round trip costs nothing anyone is waiting on.
///
/// `$1` tenant · `$2` key name.
pub const LOCK_CREDENTIAL_FOR_ACTIVATION: &str = include_str!("lock_credential_for_activation.sql");

/// The same lock, for a verb that reads nothing off the row.
///
/// Adding a registry entry is a reference PRODUCER exactly as activation is,
/// and takes the identical serialization point — the credential's row lock,
/// first — for the identical reason. It stores a reference and never opens
/// one, so it projects a constant and the row never leaves Postgres.
///
/// `$1` tenant · `$2` key name.
pub const LOCK_CREDENTIAL_FOR_REFERENCE: &str = include_str!("lock_credential_for_reference.sql");

/// The registry entry an activation points the selection at.
///
/// `DO NOTHING` because re-activating an unchanged pair is not an error and
/// must not bump anything.
///
/// The table carries three unique indexes — the `id` primary key, the
/// `(id, tenant_id)` pair the selection's foreign key targets, and the
/// `(tenant_id, model_name, secret_ref)` domain key — and `ON CONFLICT` across
/// several unique indexes is where Postgres's unprincipled deadlocks live. This
/// is safe from that class for a reason worth stating rather than leaving to
/// luck: `id` is a freshly minted uuidv7 on every call, so neither index that
/// leads with it can be the arbiter that conflicts. One index is ever in play,
/// and the hazard needs two.
///
/// `$1` id · `$2` tenant · `$3` model · `$4` secret ref.
pub const INSERT_TENANT_MODEL_IF_ABSENT: &str = include_str!("insert_tenant_model_if_absent.sql");

/// Points the selection at the entry, gated on the catalogue in one snapshot.
///
/// The gate and the write are ONE statement, so nothing can delete the model
/// between checking it and pointing at it. No row back IS the "not catalogued"
/// refusal; a separate `SELECT` first would be a race.
///
/// `$5` carries whether the credential's provider is the compatible one, and
/// it is what makes this one statement rather than two near-identical ones:
///
/// - A NAMED provider must be catalogued for its own provider. The `WHERE`
///   requires that exact row, so a miss writes nothing.
/// - The COMPATIBLE provider hosts a user's own endpoint, so the model is
///   absent from the platform catalogue by design. The `WHERE` passes
///   unconditionally.
///
/// `RETURNING` echoes the pointer and the ceiling
/// [`SELECT_TENANT_SELECTION`] would compute for it, so the response reports
/// the write instead of re-reading it — a re-read can observe a racing
/// writer's row.
///
/// `$1` tenant · `$2` provider · `$3` model · `$4` secret ref · `$5` whether
/// the provider is the compatible one.
pub const ACTIVATE_TENANT_MODEL: &str = include_str!("activate_tenant_model.sql");
