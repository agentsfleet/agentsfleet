//! SQL statements owned by the vault envelope store.

/// Store one credential: the envelope AND its non-secret projection, in one
/// statement (the metadata promotion).
///
/// The `meta_*` columns are here rather than in a follow-up UPDATE precisely so
/// they cannot disagree with the ciphertext they describe. Both the INSERT arm
/// and the ON CONFLICT arm carry all four, so an overwrite that changes the
/// provider or clears the key re-projects in the same atomic write — there is no
/// interval during which a row's stated provider belongs to its previous body.
///
/// Every value comes from one `metadata.project` call over one parse of the
/// plaintext being stored (`state/vault.zig::storeJsonPlaintext`), so no caller
/// is in a position to supply a projection of something else.
/// The envelope + projection both arms write. Shared so the create arm and the
/// rotate arm cannot come to disagree about the column set they insert.
const INSERT_SECRET_ROW =
    \\INSERT INTO vault.secrets
    \\  (id, workspace_id, key_name, encrypted_dek, dek_nonce, dek_tag, nonce, ciphertext, tag, kek_version, created_at, updated_at,
    \\   meta_kind, meta_provider, meta_base_url, meta_has_key)
    \\VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $11, $12, $13, $14, $15)
    \\
;

/// Claim a name, or report that someone already holds it.
///
/// `DO NOTHING` makes the uniqueness decision Postgres's rather than the
/// caller's: a read-then-write in the handler would leave a window in which two
/// requests both find the name free and the second silently buries the first
/// one's credential. The affected-row count is the answer — zero means the name
/// was taken, and no ciphertext was written.
///
/// Replacing a held name is `UPDATE_SECRET`; a create that finds the name
/// occupied must not quietly become one.
pub const INSERT_SECRET_IF_ABSENT = INSERT_SECRET_ROW ++
    \\ON CONFLICT (workspace_id, key_name) DO NOTHING
;

/// Replace the body of a secret this workspace already holds.
///
/// An UPDATE, deliberately not an upsert. The distinction is a safety property,
/// not a style choice: zero affected rows means the name is not held, which the
/// caller reports as 404. An upsert would instead CREATE the row — so a replace
/// racing a delete would resurrect a credential the operator just removed, and
/// claiming a name would stop being `create`'s sole job.
///
/// The row keeps its `id` and `created_at`; everything the envelope and the
/// projection describe is rewritten together in this one statement, so the
/// `meta_*` columns can never describe a body other than the ciphertext beside
/// them.
pub const UPDATE_SECRET =
    \\UPDATE vault.secrets SET
    \\       encrypted_dek = $3,
    \\       dek_nonce = $4,
    \\       dek_tag = $5,
    \\       nonce = $6,
    \\       ciphertext = $7,
    \\       tag = $8,
    \\       kek_version = $9,
    \\       updated_at = $10,
    \\       meta_kind = $11,
    \\       meta_provider = $12,
    \\       meta_base_url = $13,
    \\       meta_has_key = $14
    \\ WHERE workspace_id = $1 AND key_name = $2
;

pub const INSERT_SECRET = INSERT_SECRET_ROW ++
    \\ON CONFLICT (workspace_id, key_name) DO UPDATE
    \\SET encrypted_dek = EXCLUDED.encrypted_dek,
    \\    dek_nonce = EXCLUDED.dek_nonce,
    \\    dek_tag = EXCLUDED.dek_tag,
    \\    nonce = EXCLUDED.nonce,
    \\    ciphertext = EXCLUDED.ciphertext,
    \\    tag = EXCLUDED.tag,
    \\    kek_version = EXCLUDED.kek_version,
    \\    updated_at = EXCLUDED.updated_at,
    \\    meta_kind = EXCLUDED.meta_kind,
    \\    meta_provider = EXCLUDED.meta_provider,
    \\    meta_base_url = EXCLUDED.meta_base_url,
    \\    meta_has_key = EXCLUDED.meta_has_key
;

/// The non-secret projection for a named set of credentials, in ONE query that
/// touches no ciphertext column (the never-decrypt invariant).
///
/// This is the statement that takes the tenant Models page from up to one
/// envelope open per row to zero. It is the batch-metadata sibling of
/// `state/vault.zig::markExisting`: same index, same key set, same single
/// round-trip — it simply returns the four promoted columns alongside the
/// presence answer, because a caller that needs the provider label also needed
/// to know the row exists.
///
/// A row written before `schema/036` returns NULL metadata. The caller reports
/// it as an opaque credential; it deliberately does NOT decrypt to heal, because
/// a heal-on-read path would make "reads never decrypt" conditional on history.
/// `agentsfleetd backfill` is the one thing that fills those rows.
pub const SELECT_METADATA_FOR_KEYS =
    \\SELECT key_name, meta_kind, meta_provider, meta_base_url, meta_has_key
    \\  FROM vault.secrets
    \\ WHERE workspace_id = $1 AND key_name = ANY($2::text[])
;

/// Backfill work list: every workspace still holding a row whose projection
/// predates `schema/036`.
///
/// Returns WORKSPACES rather than rows, because the backfill decrypts through
/// the existing `loadAllForWorkspace` — one query and one Key Encryption Key
/// unwrap per workspace, and no new public decrypt surface for a one-shot
/// operator command. It re-decrypts a workspace's already-projected rows as a
/// consequence, which is the right trade for a command that runs once against a
/// development database: a narrower query would have to expose row-level
/// decryption to a caller outside `secrets/`.
///
/// Ordered so a resumed run is deterministic.
pub const SELECT_WORKSPACES_NEEDING_PROJECTION =
    \\SELECT DISTINCT workspace_id::text
    \\  FROM vault.secrets
    \\ WHERE meta_kind IS NULL
    \\ ORDER BY 1 ASC
;

/// Backfill writer: set the projection on one already-stored row without
/// touching its envelope. Used ONLY by the one-time backfill — the production
/// write path goes through `INSERT_SECRET`, which writes both together.
/// Fill the `meta_*` columns for a row that has none.
///
/// `meta_kind IS NULL` is the load-bearing clause, not a filter for speed. The
/// backfill decrypts a whole workspace up front and writes each projection some
/// time later; without the predicate, a credential ROTATED in that window — one
/// atomic write of new ciphertext AND new metadata, via
/// `vault.storeJsonPlaintext` — would then be overwritten with the projection of
/// the plaintext the sweep read before the rotation. The `meta_*` columns would
/// describe a body that no longer exists, which is precisely the drift the
/// write path makes unrepresentable by projecting in the same statement as the
/// ciphertext.
///
/// Every production writer sets `meta_kind` non-null, so a non-null value means
/// "already described by someone who saw the current body" and this sweep has
/// nothing to add. The command exists for pre-`schema/036` rows, and those are
/// exactly the rows where it is null.
pub const UPDATE_SECRET_METADATA =
    \\UPDATE vault.secrets
    \\   SET meta_kind = $3, meta_provider = $4, meta_base_url = $5, meta_has_key = $6
    \\ WHERE workspace_id = $1 AND key_name = $2 AND meta_kind IS NULL
;

pub const SELECT_SECRET =
    \\SELECT encrypted_dek, dek_nonce, dek_tag, nonce, ciphertext, tag, kek_version
    \\  FROM vault.secrets
    \\ WHERE workspace_id = $1 AND key_name = $2
;

/// The requested credentials of a workspace, ciphertext and all, in one read —
/// the lease's `secrets_map` resolve used to issue one `SELECT_SECRET` per
/// declared name. Same column block as `SELECT_SECRETS_FOR_WORKSPACE`, so the
/// one decrypt routine serves all three statements.
pub const SELECT_SECRETS_BY_NAMES =
    \\SELECT key_name, created_at,
    \\       encrypted_dek, dek_nonce, dek_tag, nonce, ciphertext, tag, kek_version
    \\  FROM vault.secrets
    \\ WHERE workspace_id = $1 AND key_name = ANY($2::text[])
;

/// Every credential in a workspace, ciphertext and all, in one read.
///
/// Column order deliberately puts `key_name` and `created_at` first so the
/// ciphertext block that follows keeps the exact shape and offsets
/// `SELECT_SECRET` uses — one decrypt routine serves both statements.
pub const SELECT_SECRETS_FOR_WORKSPACE =
    \\SELECT key_name, created_at,
    \\       encrypted_dek, dek_nonce, dek_tag, nonce, ciphertext, tag, kek_version
    \\  FROM vault.secrets
    \\ WHERE workspace_id = $1
    \\ ORDER BY key_name ASC
;
