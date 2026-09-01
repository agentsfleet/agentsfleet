//! Every statement this crate runs, and nothing else.
//!
//! Ports of `secrets/sql.zig` and `state/secret_reference_txn.zig`. What they
//! SELECT and what they predicate on is the original's; the only reshaping is
//! the string form this workspace writes statements in, and the `::uuid` casts
//! `sqlx` needs where the Zig driver sent an untyped parameter and let Postgres
//! infer (RULE NSQ — verbatim, schema-qualified).
//!
//! # Every statement carries the workspace in its predicate
//!
//! Not one of them trusts the handler to have checked first. A name belonging
//! to another workspace resolves NO ROW rather than the wrong row, which is
//! what makes the 404 in front of it honest. The ownership LAYER is a
//! capability question and this is the tenancy boundary; the two are
//! independent on purpose, and a fault in either alone still leaves the other
//! standing.
//!
//! # The projection is written by the same statement as the ciphertext
//!
//! Both write arms carry all four `meta_*` columns. Not one of them is left to
//! a follow-up UPDATE, because an interval during which a row's stated provider
//! belongs to its previous body is an interval in which the list lies. Every
//! value comes from one [`crate::SecretBody`], which produced them from the one
//! parse of the bytes being sealed.

/// The columns both write arms set, so the create arm and the replace arm
/// cannot come to disagree about the column set.
///
/// A macro expanding to a LITERAL rather than a `const`, because `concat!`
/// takes literals only and two hand-kept copies of sixteen column names would
/// drift the moment one arm gained a column (RULE UFS).
macro_rules! insert_row {
    () => {
        "INSERT INTO vault.secrets \
           (id, workspace_id, key_name, \
            encrypted_dek, dek_nonce, dek_tag, nonce, ciphertext, tag, kek_version, \
            created_at, updated_at, \
            meta_kind, meta_provider, meta_base_url, meta_has_key) \
         VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7, $8, $9, $10, $11, $11, \
                 $12, $13, $14, $15) "
    };
}

/// Claims a name, or reports that somebody already holds it.
///
/// `$1` id · `$2` workspace · `$3` name · `$4`–`$10` the envelope · `$11` now ·
/// `$12`–`$15` the projection.
///
/// `DO NOTHING` makes the uniqueness decision Postgres's rather than the
/// caller's: a read-then-write in the handler leaves a window in which two
/// requests both find the name free and the second silently buries the first
/// one's credential. The affected-row count is the answer — zero means the name
/// was taken, and no ciphertext was written.
///
/// Replacing a held name is [`UPDATE_SECRET`]; a create that finds the name
/// occupied must not quietly become one.
pub(crate) const INSERT_SECRET_IF_ABSENT: &str = concat!(
    insert_row!(),
    "ON CONFLICT (workspace_id, key_name) DO NOTHING"
);

/// Replaces the body of a secret this workspace already holds.
///
/// `$1` workspace · `$2` name · `$3`–`$9` the envelope · `$10` now ·
/// `$11`–`$14` the projection.
///
/// An UPDATE, deliberately not an upsert. The distinction is a safety property
/// rather than a style choice: zero affected rows means the name is not held,
/// which the caller reports as 404. An upsert would instead CREATE the row — so
/// a replace racing a delete would resurrect a credential the operator just
/// removed, and claiming a name would stop being create's sole job.
///
/// The row keeps its `id` and `created_at`; everything the envelope and the
/// projection describe is rewritten together, so the `meta_*` columns can never
/// describe a body other than the ciphertext beside them.
pub(crate) const UPDATE_SECRET: &str = "\
UPDATE vault.secrets SET
       encrypted_dek = $3,
       dek_nonce = $4,
       dek_tag = $5,
       nonce = $6,
       ciphertext = $7,
       tag = $8,
       kek_version = $9,
       updated_at = $10,
       meta_kind = $11,
       meta_provider = $12,
       meta_base_url = $13,
       meta_has_key = $14
 WHERE workspace_id = $1::uuid AND key_name = $2";

/// Every credential a workspace holds, as the non-secret projection alone.
///
/// `$1` workspace.
///
/// **No ciphertext column appears here, and that is the statement's whole
/// point.** `secret_list.zig` answers this page by reading every envelope and
/// projecting the decrypted body per row, which costs one key unwrap and one
/// AES-GCM open per credential on every dashboard load and puts plaintext in
/// the process for a request that displays none of it. The four `meta_*`
/// columns were promoted precisely so this read would not have to, and spec
/// Invariant 3 says a list performs zero decrypts. A projection this statement
/// cannot return is one the list does not serve — see [`crate::projection`] on
/// `model`.
///
/// Ordered by name so two pages of the same workspace read the same way, which
/// is what `SELECT_SECRETS_FOR_WORKSPACE` orders by too.
pub(crate) const SELECT_SECRET_PROJECTIONS: &str = "\
SELECT key_name, created_at, meta_kind, meta_provider, meta_base_url
  FROM vault.secrets
 WHERE workspace_id = $1::uuid
 ORDER BY key_name ASC";

/// The descriptors for a NAMED SET of credentials, in one read.
///
/// `$1` workspace · `$2` the names.
///
/// The same never-decrypt property as [`SELECT_SECRET_PROJECTIONS`] and none of
/// its shape: that statement walks a workspace, this one answers exactly the
/// names a caller already holds. The tenant model registry renders at most a
/// page of entries and would otherwise read every credential the tenant owns to
/// do it.
///
/// `meta_has_key` appears here and on no other read. Key PRESENCE is the
/// registry page's question — a credential holding no key renders differently —
/// and the secrets list has never displayed it, so the column is projected by
/// the one statement whose caller shows it.
///
/// No ORDER BY: the caller matches rows back by name, so an order would be a
/// guarantee nothing consumes.
pub(crate) const SELECT_SECRET_DESCRIPTORS: &str = "\
SELECT key_name, meta_kind, meta_provider, meta_base_url, meta_has_key
  FROM vault.secrets
 WHERE workspace_id = $1::uuid AND key_name = ANY($2::text[])";

/// The sealed envelope of one secret, for the daemon's own use.
///
/// `$1` workspace · `$2` name.
///
/// The one statement in this crate that projects ciphertext, and it is scoped
/// to a SINGLE name rather than to a workspace. That is the difference between
/// it and [`SELECT_SECRET_PROJECTIONS`]: a list cannot become a bulk decrypt by
/// accident here, because there is no shape of this statement that returns more
/// than one row. Read by `load.rs`, which is on the half of the crate that
/// holds a key; see that module on which guarantee this does and does not
/// touch.
pub(crate) const SELECT_SECRET_ENVELOPE: &str = "\
SELECT encrypted_dek, dek_nonce, dek_tag, nonce, ciphertext, tag, kek_version
  FROM vault.secrets
 WHERE workspace_id = $1::uuid AND key_name = $2";

// ── The reference lock ──────────────────────────────────────────────────────
//
// `core.tenant_model_entries.secret_ref` names a `vault.secrets` row but cannot
// be a foreign key: `secret_ref` is TEXT and the vault's identity is
// `(workspace_id, key_name)`, while an entry is keyed by tenant. So the database
// cannot refuse an entry pointing at a credential that no longer exists — only
// a lock protocol can, and only if every participant takes the same locks in
// the same order.
//
//   1. `vault.secrets (workspace_id, key_name)`          FOR UPDATE
//   2. `core.tenant_model_entries` for that ref, by id   FOR UPDATE
//   3. `core.tenant_model_selection` for the tenant      FOR UPDATE
//
// Order is the deadlock-freedom argument, and it is why these live together
// rather than being spelled at each call site: a protocol every caller
// re-implements is one that a caller eventually re-implements backwards.

/// Step 1. The credential itself, locked.
///
/// `$1` workspace · `$2` name.
///
/// `SELECT 1 … FOR UPDATE` rather than a plain read: the row lock is the entire
/// point, and zero rows means the credential is already gone.
pub(crate) const LOCK_SECRET: &str = "\
SELECT 1 FROM vault.secrets
 WHERE workspace_id = $1::uuid AND key_name = $2
   FOR UPDATE";

/// Step 0, issued after step 1 because that is the cheaper rejection.
///
/// `$1` workspace.
///
/// Whose entries are at stake is DERIVED from the workspace, never taken from
/// the caller. The credential lives in a workspace, `core.workspaces.tenant_id`
/// is `NOT NULL`, and that tenant's entries are the only ones that can name it.
/// A caller-supplied tenant answers a different question — who is asking — and
/// the two diverge exactly where it does the most damage: an operator with
/// cross-workspace authority once passed its OWN tenant here, matched no
/// entries, and deleted a credential the victim's registry still named.
pub(crate) const OWNING_TENANT: &str = "\
SELECT tenant_id::text FROM core.workspaces
 WHERE id = $1::uuid";

/// Step 2. Every entry naming this credential, locked in id order.
///
/// `$1` tenant · `$2` name.
///
/// Returns them, so a caller needing the reference count gets it from the same
/// statement that took the locks and no second read can observe a different
/// set. `ORDER BY id` is load-bearing: two writers locking one set of rows in
/// opposite orders deadlock each other.
pub(crate) const LOCK_ENTRIES: &str = "\
SELECT id::text FROM core.tenant_model_entries
 WHERE tenant_id = $1::uuid AND secret_ref = $2
 ORDER BY id
   FOR UPDATE";

/// Step 3. The tenant's active selection, locked.
///
/// `$1` tenant.
///
/// Locked even when the caller does not intend to write it: activation and
/// deletion both read it to decide, and a decision made against an unlocked row
/// is a decision made against a row that can change before the commit. Zero
/// rows is normal — a tenant that has never chosen a model — and the lock is
/// then a no-op.
pub(crate) const LOCK_SELECTION: &str = "\
SELECT 1 FROM core.tenant_model_selection
 WHERE tenant_id = $1::uuid
   FOR UPDATE";

/// Removes the credential, inside the transaction that locked it.
///
/// `$1` workspace · `$2` name.
pub(crate) const DELETE_SECRET: &str = "\
DELETE FROM vault.secrets
 WHERE workspace_id = $1::uuid AND key_name = $2";

#[cfg(test)]
mod tests;
