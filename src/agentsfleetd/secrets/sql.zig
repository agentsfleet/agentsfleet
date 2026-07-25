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
pub const INSERT_SECRET =
    \\INSERT INTO vault.secrets
    \\  (id, workspace_id, key_name, encrypted_dek, dek_nonce, dek_tag, nonce, ciphertext, tag, kek_version, created_at, updated_at,
    \\   meta_kind, meta_provider, meta_base_url, meta_has_key)
    \\VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $11, $12, $13, $14, $15)
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
pub const UPDATE_SECRET_METADATA =
    \\UPDATE vault.secrets
    \\   SET meta_kind = $3, meta_provider = $4, meta_base_url = $5, meta_has_key = $6
    \\ WHERE workspace_id = $1 AND key_name = $2
;

pub const SELECT_SECRET =
    \\SELECT encrypted_dek, dek_nonce, dek_tag, nonce, ciphertext, tag, kek_version
    \\  FROM vault.secrets
    \\ WHERE workspace_id = $1 AND key_name = $2
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
