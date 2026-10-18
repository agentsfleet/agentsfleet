//! The statements an account's members are listed and removed by.

/// The account's members, oldest membership first.
///
/// `$1` tenant. Serves both the owner's members page and a workspace's sender
/// names; the second leaves the address out when it renders, and names each
/// member by the subject their steers record.
pub const SELECT_MEMBERS: &str = include_str!("member/select_members.sql");

/// One member's role, locked until the removal commits.
///
/// `$1` tenant · `$2` user.
pub const LOCK_MEMBERSHIP: &str = include_str!("member/lock_membership.sql");

/// The account's owners, locked, so two removals cannot each see another
/// owner standing and together leave none.
///
/// Every removal takes these first, in id order, before the row it removes:
/// one order for everyone, so two removals queue instead of deadlocking. The
/// lock is the point and the count the only fact read, so each row answers a
/// constant rather than a column to decode.
///
/// `$1` tenant · `$2` the owner role's spelling.
pub const LOCK_OWNERS: &str = include_str!("member/lock_owners.sql");

/// Removes one membership.
///
/// `$1` tenant · `$2` user.
pub const DELETE_MEMBERSHIP: &str = include_str!("member/delete_membership.sql");
