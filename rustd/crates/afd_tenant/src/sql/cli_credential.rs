//! The `afc_` command-line credential statements — `core.cli_credentials`.
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
pub const INSERT_CLI_CREDENTIAL: &str = "\
INSERT INTO core.cli_credentials \
(id, user_id, tenant_id, machine_name, credential_hash, \
credential_prefix, deployment, created_from_address, created_at) \
VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, $6, $7, $8, $9)";

/// Revoke this machine's live credential ahead of minting its replacement.
///
/// Scoped to one `(user, machine)`: another machine's credential is untouched,
/// which is what lets a second laptop keep working across a re-login.
pub const REVOKE_CLI_CREDENTIAL_FOR_MACHINE: &str = "\
UPDATE core.cli_credentials \
SET revoked_at = $3 \
WHERE user_id = $1::uuid AND machine_name = $2 AND revoked_at IS NULL";

/// Revoke one credential by id, scoped to its owner.
///
/// A credential belonging to somebody else is indistinguishable from one that
/// does not exist, so an identifier cannot be probed for existence.
pub const REVOKE_CLI_CREDENTIAL_BY_ID: &str = "\
UPDATE core.cli_credentials \
SET revoked_at = $3 \
WHERE id = $1::uuid AND user_id = $2::uuid AND revoked_at IS NULL";

/// Resolve an authenticated subject to the user row these endpoints write
/// against.
///
/// `core.cli_credentials.user_id` is a foreign key to `core.users(id)`, while a
/// principal carries the identity provider's subject — so the two are one
/// lookup apart.
///
/// Deliberately narrower than the bootstrap identity read, which joins
/// memberships on the owner role and requires a named workspace: a read-only
/// collaborator satisfies neither and would resolve nothing. A collaborator
/// minting a credential for their own terminal is precisely the case the
/// resolved-capability model exists to keep working.
pub const SELECT_USER_IDENTITY_BY_SUBJECT: &str = "\
SELECT id::text, tenant_id::text \
FROM core.users \
WHERE oidc_subject = $1 \
LIMIT 1";
