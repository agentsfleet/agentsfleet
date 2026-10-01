//! The rows a suite seeds an account with, spelled once for every suite.
//!
//! An account as signup leaves it, a membership in someone else's, and the
//! cleanup that removes both. `afd_tenant`'s suites and `afd_api`'s seed the
//! same rows, so the statements live here rather than once per crate.
#![expect(
    clippy::expect_used,
    reason = "a feature-gated fixture: every expect here reports a LANE fault — an unreachable Postgres, a seed the schema refused — where a Result would hand the suite a value it could only unwrap (dispatch/write_rust.md, test-util carve-out)"
)]

use afd_db::Db;
use afd_db::test_util::mint_id;

use crate::workspace::access::ROLE_OWNER;

/// What a seed reports when the lane gives it no connection.
const NO_CONNECTION: &str = "an API connection";

/// One person's account as signup leaves it, less the wallet.
#[derive(Debug, Clone, Copy)]
pub struct Signup<'a> {
    /// The account.
    pub tenant: &'a str,
    /// The person's user row.
    pub user: &'a str,
    /// The identity provider's subject for them.
    pub subject: &'a str,
    /// Their address, as stored.
    pub email: &'a str,
    /// The account's name, and its one workspace's.
    pub name: &'a str,
    /// The person's own name, when the identity provider gave one.
    pub display_name: Option<&'a str>,
    /// The account's one workspace.
    pub workspace: &'a str,
}

/// Writes `person`'s tenant, user, owner membership and workspace, all at 1.
///
/// # Panics
/// When the lane's Postgres will not take the rows.
pub async fn sign_up(database: &Db, person: &Signup<'_>) {
    let mut connection = database.acquire().await.expect(NO_CONNECTION);
    sqlx::query(
        "WITH tenant AS ( \
           INSERT INTO core.tenants (id, name, created_at, updated_at) \
           VALUES ($1::uuid, $2, 1, 1) \
         ), person AS ( \
           INSERT INTO core.users \
             (id, tenant_id, oidc_subject, email, display_name, created_at, updated_at) \
           VALUES ($3::uuid, $1::uuid, $4, $5, $6, 1, 1) \
         ), membership AS ( \
           INSERT INTO core.memberships (id, tenant_id, user_id, role, created_at) \
           VALUES ($7::uuid, $1::uuid, $3::uuid, $8, 1) \
         ) \
         INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
         VALUES ($9::uuid, $1::uuid, $2, $4, 1)",
    )
    .bind(person.tenant)
    .bind(person.name)
    .bind(person.user)
    .bind(person.subject)
    .bind(person.email)
    .bind(person.display_name)
    .bind(mint_id())
    .bind(ROLE_OWNER)
    .bind(person.workspace)
    .execute(&mut *connection)
    .await
    .expect("a signed-up person seeds");
}

/// `user` in `tenant`'s account with `role` from time 2, whatever they held.
///
/// # Panics
/// When the lane's Postgres will not take the row.
pub async fn hold(database: &Db, tenant: &str, user: &str, role: &str) {
    let mut connection = database.acquire().await.expect(NO_CONNECTION);
    sqlx::query(
        "INSERT INTO core.memberships (id, tenant_id, user_id, role, created_at) \
         VALUES ($1::uuid, $2::uuid, $3::uuid, $4, 2) \
         ON CONFLICT (tenant_id, user_id) DO UPDATE SET role = EXCLUDED.role",
    )
    .bind(mint_id())
    .bind(tenant)
    .bind(user)
    .bind(role)
    .execute(&mut *connection)
    .await
    .expect("the membership is set");
}

/// Removes `tenants` and everything that cascades from them.
///
/// # Panics
/// When the lane's Postgres will not remove them.
pub async fn delete_accounts(database: &Db, tenants: &[&str]) {
    let mut connection = database.acquire().await.expect(NO_CONNECTION);
    sqlx::query("DELETE FROM core.tenants WHERE id = ANY($1::uuid[])")
        .bind(tenants)
        .execute(&mut *connection)
        .await
        .expect("the accounts clean up");
}
