//! The HTTP half of the install credential pre-flight: the status and the body.
//!
//! `afd_fleet_lifecycle`'s own suite proves the ordering — that a refused
//! install writes no row — and it stops at that crate's `Error`. Everything
//! between the error and the wire is unasserted there: the handler that maps
//! it, the status the registry entry carries, and the extension the operator
//! reads. Delete the mapping and answer 500 with no names and that suite still
//! passes, which is why this one exists.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use crate::harness;

use afd_auth::credential::Presented;
use afd_auth::directory::Digest;
use afd_auth::scope::{Scope, ScopeSet};
use afd_core::id::Uuid7;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::{Fleet, json_body, send};

/// The signed-in person this suite seeds.
const SUBJECT: &str = "user_live_install_credentials";

/// The credential the seeded bundle declares.
const DECLARED: &str = "github";

/// The registry code an install into a workspace short a credential earns.
const CODE: &str = "UZ-BUNDLE-003";

/// A bundle whose trigger names one credential.
const TRIGGER_MD: &str = "---\nname: needs-a-secret\nx-agentsfleet:\n  triggers:\n    - type: api\n  tools: []\n  credentials:\n    - github\n  budget:\n    daily_dollars: 1.0\n---\n";

/// Its skill document.
const SKILL_MD: &str =
    "---\nname: needs-a-secret\ndescription: Needs a secret.\nversion: 1.0.0\n---\nRun.\n";

/// The refusal names the credentials to add, and says so at 424.
#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn an_install_short_a_credential_answers_424_naming_it() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let queue = harness::connect_redis().await;
    let router = Fleet::live(
        fixture.database.clone(),
        SUBJECT,
        ScopeSet::from_scopes(&Scope::ALL),
    )
    .with_owned_workspace(fixture.workspace.clone())
    .with_approval_queue(fixture.database.clone(), queue)
    .router();

    let refused = send(
        &router,
        Method::POST,
        &format!("/v1/workspaces/{}/fleets", fixture.workspace.as_str()),
        Some(&fixture.token),
        &serde_json::json!({ "platform_library_id": fixture.library }).to_string(),
    )
    .await;
    let status = refused.status();
    let body = json_body(refused).await;

    assert_eq!(status, StatusCode::FAILED_DEPENDENCY, "{body}");
    assert_eq!(body.get("error_code").and_then(Value::as_str), Some(CODE));
    assert_eq!(
        body.get("missing_secrets").and_then(Value::as_array),
        Some(&vec![Value::String(DECLARED.to_owned())]),
        "the operator is told WHICH secret to add: {body}"
    );
    assert!(
        body.get("user_message").is_some(),
        "the dashboard renders a curated sentence for this code: {body}"
    );

    fixture.cleanup().await;
}

struct Fixture {
    lane: TestDatabase,
    database: Db,
    tenant: String,
    workspace: Uuid7,
    user: String,
    key: String,
    library: String,
    token: String,
}

impl Fixture {
    async fn create() -> Self {
        let lane = TestDatabase::shared();
        let token_bits = format!("{}{}", mint_id(), mint_id()).replace('-', "");
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            tenant: mint_id(),
            workspace: Uuid7::parse(&mint_id()).expect("a minted workspace is canonical"),
            user: mint_id(),
            key: mint_id(),
            library: mint_id(),
            token: format!("agt_t{token_bits}"),
            lane,
        }
    }

    async fn seed(&self) {
        let digest = Digest::of(&Presented::new(&self.token).expect("the token is valid"));
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "WITH tenant AS ( \
               INSERT INTO core.tenants (id, name, created_at, updated_at) \
               VALUES ($1::uuid, 'Install credentials', 1, 1) \
             ), workspace AS ( \
               INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
               VALUES ($2::uuid, $1::uuid, 'install-credentials', $3, 1) \
             ), person AS ( \
               INSERT INTO core.users \
                 (id, tenant_id, oidc_subject, email, created_at, updated_at) \
               VALUES ($4::uuid, $1::uuid, $3, 'install-credentials@example.test', 1, 1) \
             ) \
             INSERT INTO core.api_keys \
               (id, tenant_id, key_name, description, key_hash, created_by, active, \
                revoked_at, created_at, updated_at) \
             VALUES ($5::uuid, $1::uuid, 'fixture', '', $6, $3, TRUE, NULL, 1, 1)",
        )
        .bind(&self.tenant)
        .bind(self.workspace.as_str())
        .bind(SUBJECT)
        .bind(&self.user)
        .bind(&self.key)
        .bind(digest.as_str())
        .execute(&mut *connection)
        .await
        .expect("the authenticated workspace seeds");

        sqlx::query(
            "INSERT INTO core.fleet_library \
               (id, name, description, source_repo, source_path, source_ref, \
                required_credentials, required_credentials_reasons, required_tools, \
                network_hosts, visibility, content_hash, skill_markdown, trigger_markdown, \
                created_at, updated_at) \
             VALUES ($1, 'needs-a-secret', 'fixture', 'repo', 'path', 'main', $5::jsonb, '{}', \
                     '[]', '[]', 'public', $2, $3, $4, 1, 1)",
        )
        .bind(&self.library)
        .bind(format!("sha256:{}", self.library))
        .bind(SKILL_MD)
        .bind(TRIGGER_MD)
        .bind(format!("[\"{DECLARED}\"]"))
        .execute(&mut *connection)
        .await
        .expect("the credential-demanding library entry seeds");
    }

    async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query("DELETE FROM core.fleet_library WHERE id = $1")
            .bind(&self.library)
            .execute(&mut *connection)
            .await
            .expect("the library entry cleans up");
        sqlx::query("DELETE FROM core.tenants WHERE id = $1::uuid")
            .bind(&self.tenant)
            .execute(&mut *connection)
            .await
            .expect("the scoped fixture cleans up");
        drop(connection);
        drop(self.database);
        self.lane.cleanup().await;
    }
}
