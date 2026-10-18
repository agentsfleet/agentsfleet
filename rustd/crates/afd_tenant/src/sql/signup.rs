//! The statements that open a personal account. Each is a `.sql` file in
//! `signup/`, checked against the rebuilt schema by `afd_db`'s
//! `test_every_sql_file_prepares`.
//!
//! The workspace insert here is signup's OWN, and the difference from
//! [`super::workspace::INSERT_WORKSPACE`] is load-bearing rather than
//! cosmetic: `workspace::directory` retries a name collision by re-running a
//! standalone insert, while signup's retry happens INSIDE the account
//! transaction, where a raised unique violation would abort every statement
//! that came before it. `ON CONFLICT DO NOTHING` makes a collision a
//! zero-row result the loop can read instead of a poisoned transaction.

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

/// Open the default workspace, yielding to a name another draw already took.
///
/// The partial conflict target matches the partial unique index: a workspace
/// with a NULL name is not subject to per-tenant name uniqueness. See the
/// module note on why this is not `workspace::INSERT_WORKSPACE`.
pub const INSERT_WORKSPACE_IF_FREE: &str = include_str!("signup/insert_workspace_if_free.sql");
