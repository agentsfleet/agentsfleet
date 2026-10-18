//! The statements that read and rewrite `vault.secrets`, and nothing else.
//!
//! Each is a `.sql` file beside this module, schema-qualified, and checked
//! against the rebuilt schema by `afd_db`'s `test_every_sql_file_prepares`.
//! They live apart from the provider's because their CALLERS are different:
//! the provider opens one named row, and the secrets map opens the set a fleet
//! declared. [`super`] explains why the split falls on domain rather than on
//! line count.
//!
//! Every read projects the envelope by the column names
//! [`afd_vault::StoredEnvelope`] decodes, so a projection may order them as it
//! likes and carry other columns beside them.

/// One credential's envelope: the six ciphertext columns plus its version.
///
/// `workspace_id` is cast, and the cast is NOT optional. `sqlx` binds a `&str`
/// as `text` rather than leaving the parameter untyped for Postgres to infer,
/// and `vault.secrets.workspace_id` is `UUID NOT NULL`
/// (`schema/1200_vault_secrets.sql`). Without the cast the comparison arrives
/// as `uuid = text`, Postgres finds no operator, and EVERY vault read fails at
/// runtime with a query error. An earlier revision omitted it and failed in
/// exactly the way this crate is most exposed to: the unit lane never opens a
/// connection, so nothing type-checked it.
///
/// `$1` workspace, `$2` key name.
pub const SELECT_SECRET: &str = include_str!("select_secret.sql");

/// The requested credentials of a workspace, ciphertext and all, in one read.
///
/// `$1` workspace, `$2` the names.
pub const SELECT_SECRETS_BY_NAMES: &str = include_str!("select_secrets_by_names.sql");

/// The same row as [`SELECT_SECRET`], held for a read-modify-write.
///
/// The rotation write-back is the only path in this crate that WRITES a stored
/// credential, and it must not clobber a handle an administrator replaced while
/// the exchange was in flight. `FOR UPDATE` is what makes its guard real: a
/// read, compare and write with no lock at all would let a reconnect landing
/// between the read and the write be silently overwritten with a refresh token
/// belonging to the grant that was just replaced.
///
/// `$1` workspace, `$2` key name.
pub const LOCK_SECRET: &str = include_str!("lock_secret.sql");

/// Replaces one credential's envelope in place.
///
/// The envelope columns and `updated_at` only. The four `meta_*` projection
/// columns are deliberately left alone: those describe the SHAPE of the stored
/// handle — its kind, its provider, whether it carries a key — and a
/// refresh-token rotation changes none of them. Rewriting them would mean
/// re-deriving a projection from a body whose shape is known not to have
/// changed, which is a second place for the projection to come to disagree with
/// the row beside it.
///
/// `updated_at` is the database's `now()`: the lifecycle stamp is the store's,
/// not the caller's.
///
/// `$1` workspace, `$2` key name, `$3`–`$9` the envelope.
pub const UPDATE_SECRET_ENVELOPE: &str = include_str!("update_secret_envelope.sql");
