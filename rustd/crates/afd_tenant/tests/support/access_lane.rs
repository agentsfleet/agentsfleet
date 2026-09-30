//! The principals and verdicts the access suites build, spelled once.
//!
//! Every access suite asks the same resolver about the same kinds of caller:
//! a browser session naming a subject, and a claim-bound credential naming a
//! tenant. These are the constructors for both and for the verdicts they get,
//! and the rows every such suite seeds: a signed-up account and a membership.

use afd_auth::principal::{Person, PersonCredential, Principal, Subject};
use afd_auth::scope::ScopeSet;
use afd_core::id::Uuid7;
use afd_db::Db;
use afd_db::test_util::mint_id;
use afd_tenant::workspace::access::{Access, Grant, ROLE_OWNER, Role};

/// A stored or minted identifier, parsed.
pub(crate) fn id(value: &str) -> Uuid7 {
    Uuid7::parse(value).expect("the fixture identifier is UUIDv7")
}

/// A person proven by `credential`, claiming `tenant` as `subject`.
pub(crate) fn person(
    credential: PersonCredential,
    tenant: &str,
    subject: &str,
    scopes: ScopeSet,
) -> Principal {
    Principal::Person(Person::new(
        credential,
        id(tenant),
        Subject::new(subject).expect("the fixture subject is not blank"),
        scopes,
    ))
}

/// A browser session for `subject`, whose token claims `tenant`.
pub(crate) fn session(tenant: &str, subject: &str) -> Principal {
    person(
        PersonCredential::SessionToken {
            workspace_scope: None,
        },
        tenant,
        subject,
        ScopeSet::EMPTY,
    )
}

/// The verdict for `tenant`'s workspace held with `role`.
pub(crate) fn held(tenant: &str, role: Role) -> Access {
    Access {
        tenant: id(tenant),
        grant: Grant::Membership(role),
    }
}

/// One person's account as signup leaves it, less the wallet.
pub(crate) struct Signup<'a> {
    pub(crate) tenant: &'a str,
    pub(crate) user: &'a str,
    pub(crate) subject: &'a str,
    pub(crate) email: &'a str,
    /// The account's name, and its one workspace's.
    pub(crate) name: &'a str,
    /// The person's own name, when the identity provider gave one.
    pub(crate) display_name: Option<&'a str>,
    pub(crate) workspace: &'a str,
}

/// Writes `person`'s tenant, user, owner membership and workspace, all at 1.
pub(crate) async fn sign_up(database: &Db, person: &Signup<'_>) {
    let mut connection = database.acquire().await.expect("an API connection");
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
pub(crate) async fn hold(database: &Db, tenant: &str, user: &str, role: &str) {
    let mut connection = database.acquire().await.expect("an API connection");
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

/// Removes three accounts and everything that cascades from them.
pub(crate) async fn delete_accounts(database: &Db, tenants: [&str; 3]) {
    let mut connection = database.acquire().await.expect("an API connection");
    let [first, second, third] = tenants;
    sqlx::query("DELETE FROM core.tenants WHERE id IN ($1::uuid, $2::uuid, $3::uuid)")
        .bind(first)
        .bind(second)
        .bind(third)
        .execute(&mut *connection)
        .await
        .expect("the accounts clean up");
}
