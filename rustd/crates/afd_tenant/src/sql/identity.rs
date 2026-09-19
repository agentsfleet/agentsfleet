//! The one statement behind `GET /v1/users/me` — `core.users` × `core.tenants`.
//!
//! # Why this is a second statement and not a widened first one
//!
//! [`super::cli_credential::SELECT_USER_IDENTITY_BY_SUBJECT`] already resolves a
//! subject to a user row, and widening it would have cost nothing to write. It
//! would have cost something to RUN: the mint and revoke paths read two columns
//! and would then pay a join, on every login, for a display name neither reads.
//! Two statements over one table pair is cheaper than one statement doing a
//! stranger's work.
//!
//! # Why the tenant name is joined rather than looked up
//!
//! A person recognises "Ada's Workshop", not a version-7 Universally Unique
//! Identifier (UUID). Both come from one round trip because they are one answer
//! — a caller holding the identifier and not the name would have to ask again to
//! render anything, and the second ask could disagree with the first.

/// Resolve a proven identity-provider subject to the person behind it.
///
/// `LIMIT 1` is belt to `uq_users_oidc_subject`'s braces: the unique index makes
/// a second match unrepresentable, and a statement that would silently take the
/// first of several is not the shape to write beside it.
///
/// The `::text` casts are load-bearing. Both identifier columns are `UUID`, and
/// a driver reading one as text without the cast hands back raw bytes — the same
/// note `afd_billing::tenant_sql` carries for the same reason.
pub const SELECT_CALLER_PROFILE_BY_SUBJECT: &str = "\
SELECT users.id::text, users.email, users.display_name, \
users.tenant_id::text, tenants.name \
FROM core.users AS users \
JOIN core.tenants AS tenants ON tenants.id = users.tenant_id \
WHERE users.oidc_subject = $1 \
LIMIT 1";
