//! Tenants register and manage their own runners, over the production router
//! and a live Postgres.
//!
//! Each caller is a key seeded on its own tenant and a subject whose scopes the
//! harness grants, so the route's scope gate, the caller's tenant lookup and the
//! scoped statements all run as they do in production.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use std::sync::Arc;

use af_core::id::Uuid7;
use afd_auth::credential::Presented;
use afd_auth::directory::Digest;
use afd_auth::scope::{Scope, ScopeSet, TENANT_OWNER_GRANT};
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{ApiKeySeed, RunnerPolicy, RunnerSeed, TenantSeed, TestDatabase, mint_id};
use axum::Router;
use http::{HeaderName, Method, StatusCode};
use serde_json::Value;
use tokio::sync::Barrier;

use crate::harness::{Fleet, code_of, json_body, send, send_with_headers};

const TENANT_RUNNERS: &str = "/v1/tenants/me/runners";
const IDEMPOTENCY_KEY: HeaderName = HeaderName::from_static("idempotency-key");
const NOW: jiff::Timestamp = jiff::Timestamp::constant(1_760_000_000, 0);

/// One caller: a tenant, a key on it, and the subject the harness grants scopes to.
struct Caller {
    tenant: Uuid7,
    token: String,
    router: Router,
}

/// Every caller a case needs, on one shared lane, and the rows to remove after.
struct Fixture {
    lane: TestDatabase,
    database: Db,
    tenants: Vec<Uuid7>,
    platform_runners: Vec<Uuid7>,
}

impl Fixture {
    async fn create() -> Self {
        let lane = TestDatabase::shared();
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            lane,
            tenants: Vec::new(),
            platform_runners: Vec::new(),
        }
    }

    /// A tenant with a key whose subject holds `scopes`.
    async fn caller(&mut self, name: &str, scopes: &[Scope]) -> Caller {
        let tenant = mint_id();
        let key = mint_id();
        let subject = format!("user_{key}");
        let token = format!("agt_t{}{}", mint_id(), mint_id()).replace('-', "");
        let digest = Digest::of(&Presented::new(&token).expect("the token is valid"));
        let mut connection = self.database.acquire().await.expect("a connection");
        TenantSeed::new(&tenant)
            .name(name)
            .at(NOW)
            .insert(&mut *connection)
            .await
            .expect("the tenant seeds");
        ApiKeySeed::new(&key, &tenant, digest.as_str(), &subject)
            .at(NOW)
            .insert(&mut *connection)
            .await
            .expect("the key seeds");
        drop(connection);
        self.tenants.push(tenant.clone());
        let router = Fleet::live(
            self.database.clone(),
            &subject,
            ScopeSet::from_scopes(scopes),
        )
        .router();
        Caller {
            tenant,
            token,
            router,
        }
    }

    /// A platform runner seeded directly, as an operator's registration leaves it.
    async fn platform_runner(&mut self) -> Uuid7 {
        let runner = mint_id();
        let mut connection = self.database.acquire().await.expect("a connection");
        RunnerSeed::new(
            &runner,
            afd_state::sql::RUNNER_STATUS_ACTIVE,
            RunnerPolicy {
                sandbox_tier: "dev_none",
                network_policy: "deny_all_egress",
            },
        )
        .seen(NOW)
        .at(NOW)
        .insert(&mut *connection)
        .await
        .expect("the platform runner seeds");
        self.platform_runners.push(runner.clone());
        runner
    }

    /// `(platform_owned, tenant_id)` of one runner, or `None` when it is gone.
    async fn owner(&self, runner: &Uuid7) -> Option<(bool, Option<Uuid7>)> {
        sqlx::query_as("SELECT platform_owned, tenant_id FROM runner.runners WHERE id = $1")
            .bind(runner)
            .fetch_optional(&mut *self.database.acquire().await.expect("a connection"))
            .await
            .expect("the owner reads")
    }

    /// Runners a tenant owns.
    async fn tenant_runners(&self, tenant: &Uuid7) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM runner.runners WHERE tenant_id = $1")
            .bind(tenant)
            .fetch_one(&mut *self.database.acquire().await.expect("a connection"))
            .await
            .expect("the count reads")
    }

    /// The worker count stored for one runner.
    async fn worker_count(&self, runner: &Uuid7) -> i32 {
        sqlx::query_scalar("SELECT worker_count FROM runner.runners WHERE id = $1")
            .bind(runner)
            .fetch_one(&mut *self.database.acquire().await.expect("a connection"))
            .await
            .expect("the worker count reads")
    }

    /// Whether one runner's durable tasks are stored on.
    async fn durable_tasks(&self, runner: &Uuid7) -> bool {
        sqlx::query_scalar("SELECT durable_tasks_enabled FROM runner.runners WHERE id = $1")
            .bind(runner)
            .fetch_one(&mut *self.database.acquire().await.expect("a connection"))
            .await
            .expect("the flag reads")
    }

    /// The stored token digest of one runner.
    async fn token_hash(&self, runner: &Uuid7) -> String {
        sqlx::query_scalar("SELECT token_hash FROM runner.runners WHERE id = $1")
            .bind(runner)
            .fetch_one(&mut *self.database.acquire().await.expect("a connection"))
            .await
            .expect("the digest reads")
    }

    async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("a connection");
        sqlx::query("DELETE FROM runner.runners WHERE id = ANY($1)")
            .bind(&self.platform_runners)
            .execute(&mut *connection)
            .await
            .expect("the platform runners clean up");
        // A tenant's runners go with it (ON DELETE CASCADE).
        sqlx::query("DELETE FROM identity.tenants WHERE id = ANY($1)")
            .bind(&self.tenants)
            .execute(&mut *connection)
            .await
            .expect("the tenants clean up");
        drop(connection);
        drop(self.database);
        self.lane.cleanup().await;
    }
}

/// A registration body for `host`, with `labels`.
fn body(host: &str, labels: &str, durable: bool) -> String {
    format!(
        r#"{{"host_id":"{host}","assigned_policy":{{"sandbox_tier":"dev_none","network_policy":"allow_all","registry_allowlist":[],"worker_count":2,"extra_binds":[],"durable_tasks_enabled":{durable}}},"labels":[{labels}]}}"#
    )
}

async fn register(caller: &Caller, key: &str, body: &str) -> http::Response<axum::body::Body> {
    send_with_headers(
        &caller.router,
        Method::POST,
        TENANT_RUNNERS,
        Some(&caller.token),
        body,
        &[(IDEMPOTENCY_KEY, key)],
    )
    .await
}

/// Registers one runner for `caller` and answers its id and token.
async fn registered(caller: &Caller, host: &str) -> (Uuid7, String) {
    let created = register(
        caller,
        mint_id().as_str(),
        &body(host, r#""tenant""#, false),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let created = json_body(created).await;
    (
        Uuid7::parse(text(&created, "runner_id")).expect("a runner id"),
        text(&created, "runner_token").to_owned(),
    )
}

fn text<'v>(value: &'v Value, key: &str) -> &'v str {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("`{key}` in {value}"))
}

async fn patch(caller: &Caller, runner: &Uuid7, body: &str) -> http::Response<axum::body::Body> {
    send(
        &caller.router,
        Method::PATCH,
        &format!("{TENANT_RUNNERS}/{runner}"),
        Some(&caller.token),
        body,
    )
    .await
}

async fn admin_read(operator: &Caller, runner: &Uuid7) -> http::Response<axum::body::Body> {
    let path = format!("/v1/admin/runners/{runner}");
    send(
        &operator.router,
        Method::GET,
        &path,
        Some(&operator.token),
        "",
    )
    .await
}

mod management;
mod registration;
