//! Runner-token rotation through the production router and live Postgres.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use crate::harness;

use afd_auth::credential::Presented;
use afd_auth::directory::Digest;
use afd_auth::scope::{Scope, ScopeSet};
use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::TestDatabase;
use afd_runner::Runners;
use afd_wire::runner::{AssignedPolicy, NetworkPolicy, RegisterRequest, SandboxTier};
use http::{Method, StatusCode};
use std::borrow::Cow;

use self::harness::{Fleet, json_body, send};

const OPERATOR: &str = "fixture:platform-operator";
const NOW: UnixMillis = UnixMillis::from_millis(1_760_000_000_000);

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn platform_operator_rotates_runner_token_once() {
    let fixtures = Fixtures::create().await;
    fixtures.seed_operator().await;

    let runners = Runners::new(fixtures.database.clone(), Entropy::new());
    let enrolled = runners
        .register(&enrolment(), NOW)
        .await
        .expect("the runner enrols");
    let runner_id = enrolled.runner_id.as_str().to_owned();
    let old_token = enrolled.token.expose().to_owned();
    let router = Fleet::live(
        fixtures.database.clone(),
        OPERATOR,
        ScopeSet::from_scopes(&[Scope::RunnerWrite]),
    )
    .router();

    let before = send(&router, Method::GET, "/v1/runners/me", Some(&old_token), "").await;
    assert_eq!(before.status(), StatusCode::OK);

    let rotated = send(
        &router,
        Method::PATCH,
        &format!("/v1/fleets/runners/{runner_id}"),
        Some(&fixtures.operator_token),
        r#"{"action":"rotate"}"#,
    )
    .await;
    assert_eq!(rotated.status(), StatusCode::OK);
    assert_eq!(
        rotated.headers().get(http::header::CACHE_CONTROL),
        Some(&http::HeaderValue::from_static("no-store"))
    );
    let payload = json_body(rotated).await;
    assert_eq!(
        payload.get("id").and_then(serde_json::Value::as_str),
        Some(runner_id.as_str())
    );
    let new_token = payload
        .get("runner_token")
        .and_then(serde_json::Value::as_str)
        .expect("rotation reveals one replacement token")
        .to_owned();
    assert_ne!(new_token, old_token);

    let retired = send(&router, Method::GET, "/v1/runners/me", Some(&old_token), "").await;
    assert_eq!(retired.status(), StatusCode::UNAUTHORIZED);
    let replacement = send(&router, Method::GET, "/v1/runners/me", Some(&new_token), "").await;
    assert_eq!(replacement.status(), StatusCode::OK);
    assert_eq!(fixtures.rotation_actor(&runner_id).await, OPERATOR);

    fixtures.cleanup().await;
}

fn enrolment() -> RegisterRequest<'static> {
    RegisterRequest {
        host_id: Cow::Borrowed("rotate.fixture.test"),
        assigned_policy: AssignedPolicy {
            sandbox_tier: SandboxTier::DevNone,
            network_policy: NetworkPolicy::AllowAll,
            registry_allowlist: Vec::new(),
            worker_count: 1,
            extra_binds: Vec::new(),
        },
        labels: vec![Cow::Borrowed("rotation")],
    }
}

struct Fixtures {
    lane: TestDatabase,
    database: Db,
    tenant: Uuid7,
    operator_token: String,
}

impl Fixtures {
    async fn create() -> Self {
        let lane = TestDatabase::shared();
        let first = afd_db::test_util::mint_id().replace('-', "");
        let second = afd_db::test_util::mint_id().replace('-', "");
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            tenant: Uuid7::parse(&afd_db::test_util::mint_id())
                .expect("the fixture tenant id is well formed"),
            operator_token: format!("agt_t{first}{second}"),
            lane,
        }
    }

    async fn seed_operator(&self) {
        let digest =
            Digest::of(&Presented::new(&self.operator_token).expect("fixture token is valid"));
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "WITH tenant AS (\
               INSERT INTO core.tenants (id, name, created_at, updated_at) \
               VALUES ($2::uuid, 'Rotation test', 1, 1)\
             ) \
             INSERT INTO core.api_keys \
               (id, tenant_id, key_name, description, key_hash, created_by, \
                active, revoked_at, created_at, updated_at) \
             VALUES ($3::uuid, $2::uuid, 'operator', '', \
                     $1, $4, TRUE, NULL, 1, 1)",
        )
        .bind(digest.as_str())
        .bind(self.tenant.as_str())
        .bind(afd_db::test_util::mint_id())
        .bind(OPERATOR)
        .execute(&mut *connection)
        .await
        .expect("the operator credential seeds");
    }

    async fn rotation_actor(&self, runner_id: &str) -> String {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query_scalar(
            "SELECT metadata ->> 'actor_id' FROM fleet.runner_events \
             WHERE runner_id = $1::uuid AND event_type = 'runner_token_rotated'",
        )
        .bind(runner_id)
        .fetch_one(&mut *connection)
        .await
        .expect("the rotation event records its actor")
    }

    async fn cleanup(self) {
        drop(self.database);
        self.lane.cleanup().await;
    }
}
