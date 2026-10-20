//! The statements an account's members are listed and removed by.

/// The account's members, oldest membership first.
///
/// `$1` tenant. Serves both the owner's members page and a workspace's sender
/// names; the second leaves the address out when it renders, and names each
/// member by the subject their steers record.
pub const SELECT_MEMBERS: &str = include_str!("member/select_members.sql");

/// One member's role.
///
/// `$1` tenant · `$2` user.
pub const SELECT_MEMBERSHIP: &str = include_str!("member/select_membership.sql");

/// How many owners the account has.
///
/// Unlocked: `api_runtime` holds no UPDATE on the immutable memberships table,
/// and Postgres refuses a row lock without it. Two owners removing each other
/// at the same instant can therefore both pass the last-owner check; Indy
/// accepted that over locking (Oct 20, 2026, PR #778).
///
/// `$1` tenant · `$2` the owner role's spelling.
pub const COUNT_OWNERS: &str = include_str!("member/count_owners.sql");

/// Removes one membership.
///
/// `$1` tenant · `$2` user.
pub const DELETE_MEMBERSHIP: &str = include_str!("member/delete_membership.sql");
