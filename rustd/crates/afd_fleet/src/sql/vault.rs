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
/// `workspace_id` is cast, and the cast is NOT optional.
///
/// An earlier revision of this statement omitted it, on the stated grounds that
/// `vault.secrets.workspace_id` is `text`. It is not — `schema/300_vault_secrets.sql`
/// declares it `UUID NOT NULL` — and the omission was carried over from
/// `secrets/sql.zig`, where it is harmless for a reason that does not transfer:
/// the Zig driver sends an UNTYPED parameter and lets Postgres infer `uuid`,
/// while `sqlx` binds a `&str` as `text`. So the comparison arrives as
/// `uuid = text`, Postgres finds no operator, and EVERY vault read fails at
/// runtime with a query error.
///
/// It failed silently in exactly the way this crate is most exposed to: the
/// unit lane never opens a connection, so nothing type-checked it, and the
/// integration suites that would have caught it were written and not yet run.
///
/// `$1` workspace, `$2` key name.
pub const SELECT_SECRET: &str = "\
SELECT encrypted_dek, dek_nonce, dek_tag, nonce, ciphertext, tag, kek_version
FROM vault.secrets
WHERE workspace_id = $1::uuid AND key_name = $2";

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
 WHERE workspace_id = $1::uuid AND key_name = ANY($2::text[])";

/// The same row as [`SELECT_SECRET`], held for a read-modify-write.
///
/// The rotation write-back is the only path in this crate that WRITES a stored
/// credential, and it must not clobber a handle an administrator replaced while
/// the exchange was in flight. `FOR UPDATE` is what makes its guard real: the
/// Zig reads, compares and writes with no lock at all, so a reconnect landing
/// between its read and its write is silently overwritten with a refresh token
/// belonging to the grant that was just replaced.
///
/// `$1` workspace, `$2` key name.
pub const LOCK_SECRET: &str = "\
SELECT encrypted_dek, dek_nonce, dek_tag, nonce, ciphertext, tag, kek_version
FROM vault.secrets
WHERE workspace_id = $1::uuid AND key_name = $2
FOR UPDATE";

/// Replaces one credential's envelope in place.
///
/// The envelope columns and `updated_at` only. `secrets/sql.zig`'s
/// `UPDATE_SECRET` also rewrites the four `meta_*` projection columns, and this
/// deliberately does not: those describe the SHAPE of the stored handle — its
/// kind, its provider, whether it carries a key — and a refresh-token rotation
/// changes none of them. Rewriting them would mean re-deriving a projection
/// from a body whose shape is known not to have changed, which is a second
/// place for the projection to come to disagree with the row beside it.
///
/// `$1` workspace, `$2` key name, `$3`–`$9` the envelope, `$10` now.
pub const UPDATE_SECRET_ENVELOPE: &str = "\
UPDATE vault.secrets SET
       encrypted_dek = $3,
       dek_nonce = $4,
       dek_tag = $5,
       nonce = $6,
       ciphertext = $7,
       tag = $8,
       kek_version = $9,
       updated_at = $10
 WHERE workspace_id = $1::uuid AND key_name = $2";
