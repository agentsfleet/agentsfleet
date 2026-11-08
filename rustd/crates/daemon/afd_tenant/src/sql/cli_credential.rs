//! The `afc_` command-line credential statements — `identity.cli_credentials`.
//!
//! `credential_hash` is derived and never accepted: [`INSERT_CLI_CREDENTIAL`]
//! is the only writer, and the digest it binds is taken over the value the same
//! call just generated. A caller who could supply a digest would BE supplying
//! the credential, and storing a hash would protect nothing.
//!
//! # Both revokes are owner-scoped, and both touch only live rows
//!
//! `user_id` appears in the `WHERE` of each, so a caller cannot retire somebody
//! else's credential by guessing an identifier, and a re-login cannot revoke
//! every machine a person owns. `revoked_at IS NULL` appears in each for a
//! second reason: a re-revoke would otherwise overwrite the original timestamp,
//! and the audit trail would record when a credential was last asked about
//! rather than when it actually died.

/// Mint a credential.
///
/// The partial unique index on `(user_id, machine_name) WHERE revoked_at IS
/// NULL` is the guard: a caller that inserts without first revoking this
/// machine's live row fails here, rather than leaving two live credentials an
/// operator cannot tell apart.
pub const INSERT_CLI_CREDENTIAL: &str = include_str!("cli_credential/insert_cli_credential.sql");

/// Revoke this machine's live credential ahead of minting its replacement.
///
/// Scoped to one `(user, machine)`: another machine's credential is untouched,
/// which is what lets a second laptop keep working across a re-login.
pub const REVOKE_CLI_CREDENTIAL_FOR_MACHINE: &str =
    include_str!("cli_credential/revoke_cli_credential_for_machine.sql");

/// Revoke one credential by id, scoped to its owner.
///
/// A credential belonging to somebody else is indistinguishable from one that
/// does not exist, so an identifier cannot be probed for existence.
pub const REVOKE_CLI_CREDENTIAL_BY_ID: &str =
    include_str!("cli_credential/revoke_cli_credential_by_id.sql");

/// Resolve an authenticated subject to the user row these endpoints write
/// against.
///
/// `identity.cli_credentials.user_id` is a foreign key to `identity.users(id)`, while a
/// principal carries the identity provider's subject — so the two are one
/// lookup apart.
///
/// Deliberately narrower than the bootstrap identity read, which joins
/// memberships on the owner role and requires a named workspace: a read-only
/// collaborator satisfies neither and would resolve nothing. A collaborator
/// minting a credential for their own terminal is precisely the case the
/// resolved-capability model exists to keep working.
/// The tenant is JOINED because a person recognises "Ada's Workshop" and not a
/// version-7 Universally Unique Identifier (UUID). `GET /v1/users/me` renders
/// both, and the mint and revoke paths pay one extra index lookup per login for
/// it — cheaper than a second statement that says almost the same thing.
///
/// The `::text` casts are load-bearing: both identifier columns are `UUID`, and
/// a driver reading one as text without the cast hands back raw bytes.
pub const SELECT_USER_IDENTITY_BY_SUBJECT: &str =
    include_str!("cli_credential/select_user_identity_by_subject.sql");
