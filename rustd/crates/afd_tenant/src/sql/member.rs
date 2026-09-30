//! The statements an account's members are listed and removed by.

/// The account's members, oldest membership first.
///
/// `$1` tenant. Serves both the owner's members page and a workspace's sender
/// names; the second leaves the address out when it renders.
pub const SELECT_MEMBERS: &str = "\
SELECT u.id::text, u.display_name, u.email, m.role, m.created_at \
FROM core.memberships m \
JOIN core.users u ON u.id = m.user_id \
WHERE m.tenant_id = $1::uuid \
ORDER BY m.created_at, m.id";

/// One member's role, locked until the removal commits.
///
/// `$1` tenant · `$2` user.
pub const LOCK_MEMBERSHIP: &str = "\
SELECT role FROM core.memberships \
WHERE tenant_id = $1::uuid AND user_id = $2::uuid \
FOR UPDATE";

/// The account's owners, locked, so two removals cannot each see another
/// owner standing and together leave none.
///
/// `$1` tenant · `$2` the owner role's spelling.
pub const LOCK_OWNERS: &str = "\
SELECT id::text FROM core.memberships \
WHERE tenant_id = $1::uuid AND role = $2 \
FOR UPDATE";

/// Removes one membership.
///
/// `$1` tenant · `$2` user.
pub const DELETE_MEMBERSHIP: &str = "\
DELETE FROM core.memberships WHERE tenant_id = $1::uuid AND user_id = $2::uuid";
