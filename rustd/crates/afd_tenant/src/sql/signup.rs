//! The statements that open a personal account. Each is a `.sql` file in
//! `signup/`, checked against the rebuilt schema by `afd_db`'s
//! `test_every_sql_file_prepares`.
//!
//! The default workspace goes in through [`super::workspace::INSERT_WORKSPACE`]:
//! the tenant it names was opened in the same transaction and holds no
//! workspace, so its one generated name has nothing to collide with.

/// Resolve an authenticated subject to its owned tenant and first workspace.
///
/// Joins through `memberships` on the owner role, so a member of somebody
/// else's tenant resolves nothing rather than the wrong workspace. `$2` is the
/// role rather than a literal, so it cannot drift from the role the membership
/// insert writes.
pub const SELECT_EXISTING: &str = include_str!("signup/select_existing.sql");

/// Open the tenant a personal account hangs from.
pub const INSERT_TENANT: &str = include_str!("signup/insert_tenant.sql");

/// Open the person. `oidc_subject` carries the unique index that makes the
/// whole bootstrap idempotent under concurrency.
pub const INSERT_USER: &str = include_str!("signup/insert_user.sql");

/// Link the person to the tenant as its owner.
pub const INSERT_MEMBERSHIP: &str = include_str!("signup/insert_membership.sql");

/// Open the tenant's wallet with its one-time starter balance.
///
/// `DO NOTHING` is what makes this safe to re-run: a replay heals a wallet that
/// went missing without resetting a balance somebody has already spent down.
pub const INSERT_WALLET: &str = include_str!("signup/insert_wallet.sql");
