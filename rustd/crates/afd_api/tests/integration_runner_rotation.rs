//! Runner-token rotation through the production router and live Postgres.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "integration preconditions should fail the test loudly"
)]

mod harness;

use std::borrow::Cow;
use std::sync::atomic::{AtomicU32, Ordering};

use afd_auth::credential::Presented;
use afd_auth::directory::Digest;
use afd_auth::scope::{Scope, ScopeSet};
use afd_core::clock::UnixMillis;
use afd_core::env::MapEnv;
use afd_crypto::entropy::Entropy;
use afd_db::config::{DbRole, PoolConfig};
use afd_db::{Db, Migrator};
use afd_fleet::Runners;
use afd_wire::runner::{AssignedPolicy, NetworkPolicy, RegisterRequest, SandboxTier};
use http::{Method, StatusCode};
use sqlx::AssertSqlSafe;

use self::harness::{Fleet, json_body, send};

const LANE_KNOB: &str = "TEST_DATABASE_URL";
const OPERATOR: &str = "fixture:platform-operator";
const OPERATOR_TOKEN: &str =
    "agt_t0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const NOW: UnixMillis = UnixMillis::from_millis(1_760_000_000_000);

static SEQUENCE: AtomicU32 = AtomicU32::new(0);

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
        Some(OPERATOR_TOKEN),
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
    base_url: String,
    name: String,
    database: Db,
}

impl Fixtures {
    async fn create() -> Self {
        let base_url = std::env::var(LANE_KNOB).unwrap_or_else(|_error| {
            panic!("{LANE_KNOB} is unset — run through `make test-integration-rustd`")
        });
        let name = format!(
            "afd_api_rotation_{}_{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );
        admin(&base_url, AssertSqlSafe(format!("CREATE DATABASE {name}"))).await;
        let url = database_url(&base_url, &name);
        let migrator = open(&url, DbRole::Migrator).await;
        Migrator::new()
            .run(&migrator)
            .await
            .expect("the schema applies");
        drop(migrator);
        Self {
            database: open(&url, DbRole::Api).await,
            base_url,
            name,
        }
    }

    async fn seed_operator(&self) {
        let digest = Digest::of(&Presented::new(OPERATOR_TOKEN).expect("fixture token is valid"));
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "WITH tenant AS (\
               INSERT INTO core.tenants (id, name, created_at, updated_at) \
               VALUES ('019329c5-0000-7000-8000-000000000001', 'Rotation test', 1, 1)\
             ) \
             INSERT INTO core.api_keys \
               (id, tenant_id, key_name, description, key_hash, created_by, \
                active, revoked_at, created_at, updated_at) \
             VALUES ('019329c5-0000-7000-8000-000000000002', \
                     '019329c5-0000-7000-8000-000000000001', 'operator', '', \
                     $1, $2, TRUE, NULL, 1, 1)",
        )
        .bind(digest.as_str())
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
        admin(
            &self.base_url,
            AssertSqlSafe(format!(
                "DROP DATABASE IF EXISTS {} WITH (FORCE)",
                self.name
            )),
        )
        .await;
    }
}

fn database_url(base_url: &str, name: &str) -> String {
    let (prefix, tail) = base_url
        .rsplit_once('/')
        .expect("a Postgres URL has a database path");
    let query = tail.split_once('?').map_or("", |(_, query)| query);
    if query.is_empty() {
        format!("{prefix}/{name}")
    } else {
        format!("{prefix}/{name}?{query}")
    }
}

async fn open(url: &str, role: DbRole) -> Db {
    let env = MapEnv::from_pairs(DbRole::ALL.iter().map(|each| (each.url_knob(), url)));
    Db::connect(&PoolConfig::resolve(&env, role).expect("the URL resolves"))
        .await
        .expect("the database accepts a connection")
}

async fn admin(base_url: &str, statement: AssertSqlSafe<String>) {
    let pool = sqlx::PgPool::connect(base_url)
        .await
        .expect("the lane database is reachable");
    let mut connection = pool.acquire().await.expect("an admin connection");
    sqlx::query(statement)
        .execute(&mut *connection)
        .await
        .expect("the admin statement runs");
    drop(connection);
    pool.close().await;
}
