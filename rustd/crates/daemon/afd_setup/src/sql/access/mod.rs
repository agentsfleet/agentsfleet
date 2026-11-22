//! The authenticated subject must still belong to the workspace owner's account.

/// Locks only the person's row, through `identity.lock_user`: users,
/// workspaces and memberships are immutable and `api_runtime` holds no UPDATE
/// on them (schema rule 9), and Postgres refuses any row lock without UPDATE.
/// KEY SHARE is enough, because the only change an immutable row takes is a
/// delete. A workspace delete still waits for a write, through the draft's
/// foreign key.
///
/// `$1` workspace, `$2` authenticated identity-provider subject.
pub(crate) const PERSON: &str = include_str!("person.sql");

/// Read before touching private state, so a committed revocation wins before
/// replay. Not locked, for the reason [`PERSON`] gives: a revocation that
/// commits while this request runs does not wait for it.
///
/// `$1` owner tenant, `$2` person.
pub(crate) const MEMBER: &str = include_str!("member.sql");

/// The sign-in subject a person id names, for a runner path that holds only the id.
///
/// `$1` person.
pub(crate) const SUBJECT: &str = include_str!("subject.sql");
