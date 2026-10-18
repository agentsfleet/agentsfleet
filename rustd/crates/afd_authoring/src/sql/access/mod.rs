//! The authenticated subject must still belong to the workspace owner's account.

/// `$1` workspace, `$2` authenticated identity-provider subject.
pub(crate) const PERSON: &str = include_str!("person.sql");

/// Joined membership is locked before touching private state; a committed
/// revocation wins before replay, and a later revocation waits for this request.
///
/// `$1` owner tenant, `$2` person.
pub(crate) const MEMBER: &str = include_str!("member.sql");

/// The sign-in subject a person id names, for a runner path that holds only the id.
///
/// `$1` person.
pub(crate) const SUBJECT: &str = include_str!("subject.sql");
