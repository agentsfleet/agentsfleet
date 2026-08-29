//! Workspace ownership decisions against the migrated schema.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_auth::principal::{Person, PersonCredential, Principal, Runner, Subject};
use afd_auth::scope::{Scope, ScopeSet};
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_tenant::workspace::Workspaces;

const SUBJECT: &str = "user_workspace_ownership";
const UNKNOWN_SUBJECT: &str = "user_workspace_ownership_absent";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn ownership_preserves_credential_authority_and_platform_override() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let workspaces = Workspaces::new(fixture.database.clone(), Entropy::new());

    verify_machine_and_terminal_credentials(&workspaces, &fixture).await;
    verify_session_authority(&workspaces, &fixture).await;
    verify_platform_override(&workspaces, &fixture).await;

    fixture.cleanup().await;
}

async fn verify_machine_and_terminal_credentials(workspaces: &Workspaces, fixture: &Fixture) {
    let runner = Principal::Runner(Runner::new(id(&mint_id()), false));
    assert_eq!(
        workspaces
            .authorize(&runner, &id(&fixture.own_workspace))
            .await
            .expect("runner denial is available"),
        None
    );
    assert_eq!(
        workspaces
            .tenant_of(&runner)
            .await
            .expect("runner tenant denial is available"),
        None
    );

    for credential in [
        PersonCredential::TenantApiKey,
        PersonCredential::CliCredential,
    ] {
        let principal = person(credential, &fixture.own_tenant, SUBJECT, ScopeSet::EMPTY);
        assert_eq!(
            workspaces
                .authorize(&principal, &id(&fixture.own_workspace))
                .await
                .expect("owned workspace resolves"),
            Some(id(&fixture.own_tenant))
        );
        assert_eq!(
            workspaces
                .authorize(&principal, &id(&fixture.other_workspace))
                .await
                .expect("foreign workspace denial resolves"),
            None
        );
        assert_eq!(
            workspaces
                .tenant_of(&principal)
                .await
                .expect("credential tenant resolves without a query"),
            Some(id(&fixture.own_tenant))
        );
    }
}

async fn verify_session_authority(workspaces: &Workspaces, fixture: &Fixture) {
    let confined_elsewhere = person(
        PersonCredential::SessionToken {
            workspace_scope: Some(id(&fixture.other_workspace)),
        },
        &fixture.own_tenant,
        SUBJECT,
        ScopeSet::EMPTY,
    );
    assert_eq!(
        workspaces
            .authorize(&confined_elsewhere, &id(&fixture.own_workspace))
            .await
            .expect("workspace ceiling denial resolves"),
        None
    );

    let session = person(
        PersonCredential::SessionToken {
            workspace_scope: None,
        },
        &fixture.other_tenant,
        SUBJECT,
        ScopeSet::EMPTY,
    );
    assert_eq!(
        workspaces
            .authorize(&session, &id(&fixture.own_workspace))
            .await
            .expect("session workspace resolves"),
        Some(id(&fixture.own_tenant)),
        "the user row outranks the stale tenant claim"
    );
    assert_eq!(
        workspaces
            .tenant_of(&session)
            .await
            .expect("session tenant resolves"),
        Some(id(&fixture.own_tenant))
    );

    let claim_fallback = person(
        PersonCredential::SessionToken {
            workspace_scope: None,
        },
        &fixture.own_tenant,
        UNKNOWN_SUBJECT,
        ScopeSet::EMPTY,
    );
    assert_eq!(
        workspaces
            .tenant_of(&claim_fallback)
            .await
            .expect("an absent user falls back to the claim"),
        Some(id(&fixture.own_tenant))
    );
}

async fn verify_platform_override(workspaces: &Workspaces, fixture: &Fixture) {
    let operator = person(
        PersonCredential::SessionToken {
            workspace_scope: None,
        },
        &fixture.own_tenant,
        SUBJECT,
        ScopeSet::from_scopes(&[Scope::WorkspaceAny]),
    );
    assert_eq!(
        workspaces
            .authorize(&operator, &id(&fixture.other_workspace))
            .await
            .expect("platform override resolves"),
        Some(id(&fixture.other_tenant))
    );
    assert_eq!(
        workspaces
            .authorize(&operator, &id(&mint_id()))
            .await
            .expect("an absent override target resolves"),
        None
    );
}

fn person(
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

fn id(value: &str) -> Uuid7 {
    Uuid7::parse(value).expect("the fixture identifier is UUIDv7")
}

struct Fixture {
    lane: TestDatabase,
    database: afd_db::Db,
    own_tenant: String,
    other_tenant: String,
    own_workspace: String,
    other_workspace: String,
}

impl Fixture {
    async fn create() -> Self {
        let lane = TestDatabase::shared();
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            own_tenant: mint_id(),
            other_tenant: mint_id(),
            own_workspace: mint_id(),
            other_workspace: mint_id(),
            lane,
        }
    }

    async fn seed(&self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "WITH tenants AS ( \
               INSERT INTO core.tenants (id, name, created_at, updated_at) \
               VALUES ($1::uuid, 'own', 1, 1), ($2::uuid, 'other', 1, 1) \
             ), own_workspace AS ( \
               INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
               VALUES ($3::uuid, $1::uuid, 'own', $5, 1) \
             ), other_workspace AS ( \
               INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
               VALUES ($4::uuid, $2::uuid, 'other', $5, 1) \
             ) \
             INSERT INTO core.users \
               (id, tenant_id, oidc_subject, email, display_name, created_at, updated_at) \
             VALUES ($6::uuid, $1::uuid, $5, 'fixture@example.test', NULL, 1, 1)",
        )
        .bind(&self.own_tenant)
        .bind(&self.other_tenant)
        .bind(&self.own_workspace)
        .bind(&self.other_workspace)
        .bind(SUBJECT)
        .bind(mint_id())
        .execute(&mut *connection)
        .await
        .expect("the ownership fixture seeds atomically");
    }

    async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query("DELETE FROM core.tenants WHERE id IN ($1::uuid, $2::uuid)")
            .bind(&self.own_tenant)
            .bind(&self.other_tenant)
            .execute(&mut *connection)
            .await
            .expect("the ownership fixture cleans up");
        drop(connection);
        drop(self.database);
        self.lane.cleanup().await;
    }
}
