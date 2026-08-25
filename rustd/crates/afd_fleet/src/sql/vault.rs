//! The two statements that read `vault.secrets`, and nothing else.
//!
//! Both are copied from `secrets/sql.zig`, which is the one part of this port's
//! SQL that was already collected upstream. They live in their own module
//! rather than beside the provider's because their CALLERS are different: the
//! provider opens one named row, and the secrets map opens the set a fleet
//! declared. [`super`] explains why the split falls on domain rather than on
//! line count.

/// One credential's envelope, as the six ciphertext columns plus its version.
///
/// Column order is load-bearing and copied rather than tidied: it is the order
/// `crypto_store.zig::openEnvelopeAt` reads, which is the order
/// [`afd_crypto::envelope::Envelope::from_parts`] takes its arguments in.
///
/// `workspace_id` is compared with no `::uuid` cast, which is odd beside the
/// three statements above and is odd in `secrets/sql.zig` too. It is left alone
/// — the column is `text` in that table, and adding a cast here would change
/// which index Postgres picks for the read on every lease.
///
/// `$1` workspace, `$2` key name.
pub const SELECT_SECRET: &str = "\
SELECT encrypted_dek, dek_nonce, dek_tag, nonce, ciphertext, tag, kek_version
FROM vault.secrets
WHERE workspace_id = $1 AND key_name = $2";

/// The requested credentials of a workspace, ciphertext and all, in one read.
///
/// Column order deliberately puts `key_name` and `created_at` FIRST so the
/// ciphertext block that follows keeps the exact shape and offsets
/// [`SELECT_SECRET`] uses — which is what lets one decrypt routine serve both
/// statements, and why `Vault::decrypt` takes the block's starting index rather
/// than hard-coding zero.
///
/// `created_at` is projected and unread here. It stays because the statement is
/// shared with the credential-list endpoint that displays it, and narrowing the
/// projection would fork one statement into two.
///
/// `$1` workspace, `$2` the names.
pub const SELECT_SECRETS_BY_NAMES: &str = "\
SELECT key_name, created_at,
       encrypted_dek, dek_nonce, dek_tag, nonce, ciphertext, tag, kek_version
  FROM vault.secrets
 WHERE workspace_id = $1 AND key_name = ANY($2::text[])";
