//! SQL statements owned by the vault envelope store.

pub const INSERT_SECRET =
    \\INSERT INTO vault.secrets
    \\  (id, workspace_id, key_name, encrypted_dek, dek_nonce, dek_tag, nonce, ciphertext, tag, kek_version, created_at, updated_at)
    \\VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $11)
    \\ON CONFLICT (workspace_id, key_name) DO UPDATE
    \\SET encrypted_dek = EXCLUDED.encrypted_dek,
    \\    dek_nonce = EXCLUDED.dek_nonce,
    \\    dek_tag = EXCLUDED.dek_tag,
    \\    nonce = EXCLUDED.nonce,
    \\    ciphertext = EXCLUDED.ciphertext,
    \\    tag = EXCLUDED.tag,
    \\    kek_version = EXCLUDED.kek_version,
    \\    updated_at = EXCLUDED.updated_at
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
