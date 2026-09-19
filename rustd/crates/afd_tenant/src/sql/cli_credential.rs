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
/// The tenant is JOINED because a person recognises "Ada's Workshop" and not a
/// version-7 Universally Unique Identifier (UUID). `GET /v1/users/me` renders
/// both, and the mint and revoke paths pay one extra index lookup per login for
/// it — cheaper than a second statement that says almost the same thing.
///
/// The `::text` casts are load-bearing: both identifier columns are `UUID`, and
/// a driver reading one as text without the cast hands back raw bytes.
pub const SELECT_USER_IDENTITY_BY_SUBJECT: &str = "\
SELECT users.id::text, users.tenant_id::text, users.email, \
users.display_name, tenants.name \
FROM core.users AS users \
JOIN core.tenants AS tenants ON tenants.id = users.tenant_id \
WHERE users.oidc_subject = $1 \
LIMIT 1";

#[cfg(test)]
mod tests {
    use super::SELECT_USER_IDENTITY_BY_SUBJECT;

    /// One statement serves three callers, so it carries what the widest needs.
    ///
    /// The mint and the revoke read the identifier pair; `GET /v1/users/me`
    /// renders the address and both names. A second statement for the read would
    /// have been two spellings of "who is this subject" — this is the test that
    /// notices if somebody narrows this one back and breaks the render.
    #[test]
    fn the_subject_lookup_carries_every_field_its_callers_render() {
        for column in [
            "users.id::text",
            "users.tenant_id::text",
            "users.email",
            "users.display_name",
            "tenants.name",
        ] {
            assert!(
                SELECT_USER_IDENTITY_BY_SUBJECT.contains(column),
                "the subject lookup must select {column}"
            );
        }
        assert!(
            SELECT_USER_IDENTITY_BY_SUBJECT.contains("JOIN core.tenants"),
            "the tenant NAME comes from the join, not from a second round trip"
        );
    }

    /// It keys on the indexed subject column, so it never scans.
    #[test]
    fn the_subject_lookup_keys_on_the_unique_index() {
        assert!(SELECT_USER_IDENTITY_BY_SUBJECT.contains("oidc_subject = $1"));
    }
}
