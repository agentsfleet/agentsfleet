//! §2 of M246: tenants register and manage their own runners, over the
//! production router and a live Postgres.
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

// Value: protects=a tenant's registration is owned by that tenant, and a
// platform registration by the platform; fails_when=the row's owner is never
// set, or set from a missing tenant; why_new=§2's tenant route; seam=none.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn tenant_registers_a_tenant_runner() {
    let mut fixture = Fixture::create().await;
    let owner = fixture
        .caller("Support triage", &[Scope::RunnerTenant])
        .await;

    let created = register(
        &owner,
        mint_id().as_str(),
        &body("build-box-1", r#""tenant""#, false),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    assert_eq!(
        created
            .headers()
            .get(http::header::CACHE_CONTROL)
            .map(|v| v.as_bytes()),
        Some(b"no-store".as_slice()),
        "the token response is never cached"
    );
    let created = json_body(created).await;
    assert!(text(&created, "runner_token").starts_with("agt_r"));
    let runner = Uuid7::parse(text(&created, "runner_id")).expect("a runner id");
    assert_eq!(
        fixture.owner(&runner).await,
        Some((false, Some(owner.tenant.clone())))
    );

    let platform = fixture.platform_runner().await;
    assert_eq!(fixture.owner(&platform).await, Some((true, None)));
    fixture.cleanup().await;
}

// Value: protects=registration needs `runner:tenant`, which signup never grants;
// fails_when=the default owner grant or a runner token registers; why_new=the
// scope; seam=none.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn registration_without_runner_tenant_is_refused() {
    let mut fixture = Fixture::create().await;
    let signup = fixture
        .caller("Pull request review", &TENANT_OWNER_GRANT)
        .await;

    let refused = register(
        &signup,
        mint_id().as_str(),
        &body("refused-box", r#""tenant""#, false),
    )
    .await;
    assert_eq!(refused.status(), StatusCode::FORBIDDEN);
    assert_eq!(code_of(refused).await, "UZ-AUTH-022");
    assert_eq!(fixture.tenant_runners(&signup.tenant).await, 0);

    let owner = fixture
        .caller("Support triage", &[Scope::RunnerTenant])
        .await;
    let (_, runner_token) = registered(&owner, "runner-box").await;
    let as_runner = send_with_headers(
        &owner.router,
        Method::POST,
        TENANT_RUNNERS,
        Some(&runner_token),
        &body("from-a-runner", r#""tenant""#, false),
        &[(IDEMPOTENCY_KEY, mint_id().as_str())],
    )
    .await;
    assert_eq!(as_runner.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(fixture.tenant_runners(&owner.tenant).await, 1);
    fixture.cleanup().await;
}

// Value: protects=a retried registration never makes a second runner and never
// replays the token; fails_when=the receipt is skipped, keyed per body, or the
// replay answers the token; why_new=the registration receipt; seam=none.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn registration_retry_replays_without_the_token() {
    let mut fixture = Fixture::create().await;
    let owner = fixture
        .caller("Support triage", &[Scope::RunnerTenant])
        .await;
    let key = mint_id();
    let request = body("retry-box", r#""tenant""#, false);

    let first = json_body(register(&owner, key.as_str(), &request).await).await;
    let replay = register(&owner, key.as_str(), &request).await;
    assert_eq!(replay.status(), StatusCode::OK);
    let replay = json_body(replay).await;
    assert_eq!(text(&replay, "runner_id"), text(&first, "runner_id"));
    assert!(
        replay.get("runner_token").is_none(),
        "a replay never carries the token: {replay}"
    );
    assert_eq!(fixture.tenant_runners(&owner.tenant).await, 1);

    let other = register(
        &owner,
        key.as_str(),
        &body("retry-box", r#""other""#, false),
    )
    .await;
    assert_eq!(other.status(), StatusCode::CONFLICT);
    assert_eq!(code_of(other).await, "UZ-RUN-025");

    let missing = send(
        &owner.router,
        Method::POST,
        TENANT_RUNNERS,
        Some(&owner.token),
        &request,
    )
    .await;
    assert_eq!(missing.status(), StatusCode::BAD_REQUEST);
    assert_eq!(code_of(missing).await, "UZ-REQ-001");
    let not_v7 = register(&owner, "0b6b2d7e-0c0e-4b8a-9d47-1b2a3c4d5e6f", &request).await;
    assert_eq!(not_v7.status(), StatusCode::BAD_REQUEST);

    let model = format!(
        r#"{{"host_id":"model-box","assigned_policy":{{"sandbox_tier":"dev_none","network_policy":"allow_all","registry_allowlist":[],"worker_count":2,"extra_binds":[],"durable_tasks_enabled":false,"decision_model_id":"{}"}},"labels":[]}}"#,
        mint_id()
    );
    let with_model = register(&owner, mint_id().as_str(), &model).await;
    assert_eq!(with_model.status(), StatusCode::BAD_REQUEST);
    assert_eq!(code_of(with_model).await, "UZ-REQ-001");
    assert_eq!(fixture.tenant_runners(&owner.tenant).await, 1);
    fixture.cleanup().await;
}

// Value: protects=concurrent retries of one key register exactly once;
// fails_when=the receipt insert and the runner insert are not one transaction
// behind a unique index; why_new=the receipt; seam=none (TEST-CONTEND).
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn concurrent_registrations_with_one_key_leave_one_runner() {
    const CALLERS: usize = 100;
    let mut fixture = Fixture::create().await;
    let owner = Arc::new(
        fixture
            .caller("Support triage", &[Scope::RunnerTenant])
            .await,
    );
    let key = mint_id().as_str().to_owned();
    let barrier = Arc::new(Barrier::new(CALLERS));
    let mut tasks = Vec::with_capacity(CALLERS);
    for _ in 0..CALLERS {
        let (owner, key, barrier) = (Arc::clone(&owner), key.clone(), Arc::clone(&barrier));
        tasks.push(tokio::spawn(async move {
            barrier.wait().await;
            let answer = register(&owner, &key, &body("contended-box", r#""tenant""#, false)).await;
            let status = answer.status();
            (
                status,
                text(&json_body(answer).await, "runner_id").to_owned(),
            )
        }));
    }
    let mut created = 0;
    let mut replayed = 0;
    let mut runners = std::collections::BTreeSet::new();
    for task in tasks {
        let (status, runner) = task.await.expect("the registration task finishes");
        match status {
            StatusCode::CREATED => created += 1,
            StatusCode::OK => replayed += 1,
            other => panic!("unexpected {other}"),
        }
        runners.insert(runner);
    }
    assert_eq!((created, replayed), (1, CALLERS - 1));
    assert_eq!(runners.len(), 1, "every answer names the same runner");
    assert_eq!(fixture.tenant_runners(&owner.tenant).await, 1);
    fixture.cleanup().await;
}

// Value: protects=a tenant sees and changes only its own runners; fails_when=a
// scoped statement binds no owner, or binds one that matches everything;
// why_new=RunnerScope; seam=none.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn tenant_lists_and_manages_only_its_runners() {
    let mut fixture = Fixture::create().await;
    let a = fixture
        .caller("Support triage", &[Scope::RunnerTenant])
        .await;
    let b = fixture
        .caller("Pull request review", &[Scope::RunnerTenant])
        .await;
    let (a1, _) = registered(&a, "a-one").await;
    let (a2, _) = registered(&a, "a-two").await;
    let (b1, b_token) = registered(&b, "b-one").await;
    fixture.platform_runner().await;

    let listed =
        json_body(send(&a.router, Method::GET, TENANT_RUNNERS, Some(&a.token), "").await).await;
    let mut ids: Vec<&str> = listed["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| text(item, "id"))
        .collect();
    ids.sort_unstable();
    let mut expected = vec![a1.as_str(), a2.as_str()];
    expected.sort_unstable();
    assert_eq!(ids, expected);
    assert_eq!(listed["total"].as_i64(), Some(2));
    let page = json_body(
        send(
            &a.router,
            Method::GET,
            &format!("{TENANT_RUNNERS}?limit=1"),
            Some(&a.token),
            "",
        )
        .await,
    )
    .await;
    assert_eq!(page["items"].as_array().map(Vec::len), Some(1));

    let own = format!("{TENANT_RUNNERS}/{a1}");
    assert_eq!(
        send(&a.router, Method::GET, &own, Some(&a.token), "")
            .await
            .status(),
        StatusCode::OK
    );
    for edit in [
        r#"{"action":"cordon"}"#,
        r#"{"labels":["tenant","gpu"]}"#,
        r#"{"action":"self_test"}"#,
    ] {
        assert_eq!(
            patch(&a, &a1, edit).await.status(),
            StatusCode::OK,
            "{edit}"
        );
    }
    let first = json_body(patch(&a, &a1, r#"{"action":"rotate"}"#).await).await;
    let first_token = text(&first, "runner_token").to_owned();
    assert_eq!(
        patch(&a, &a1, r#"{"action":"rotate"}"#).await.status(),
        StatusCode::OK
    );
    let retired = send(
        &a.router,
        Method::GET,
        "/v1/runners/me",
        Some(&first_token),
        "",
    )
    .await;
    assert_eq!(
        retired.status(),
        StatusCode::UNAUTHORIZED,
        "a second rotate kills the first token"
    );

    let b_digest = fixture.token_hash(&b1).await;
    let foreign = format!("{TENANT_RUNNERS}/{b1}");
    let refusals = [
        send(&a.router, Method::GET, &foreign, Some(&a.token), "").await,
        patch(&a, &b1, r#"{"action":"cordon"}"#).await,
        patch(&a, &b1, r#"{"labels":["taken"]}"#).await,
        patch(&a, &b1, r#"{"action":"self_test"}"#).await,
        patch(&a, &b1, r#"{"action":"rotate"}"#).await,
        send(&a.router, Method::DELETE, &foreign, Some(&a.token), "").await,
    ];
    for refused in refusals {
        assert_eq!(refused.status(), StatusCode::NOT_FOUND);
        assert_eq!(code_of(refused).await, "UZ-RUN-014");
    }
    assert_eq!(
        fixture.token_hash(&b1).await,
        b_digest,
        "B's token is untouched"
    );
    assert_eq!(
        send(&b.router, Method::GET, "/v1/runners/me", Some(&b_token), "")
            .await
            .status(),
        StatusCode::OK
    );

    assert_eq!(
        patch(&a, &a2, r#"{"action":"revoke"}"#).await.status(),
        StatusCode::OK
    );
    let deleted = send(
        &a.router,
        Method::DELETE,
        &format!("{TENANT_RUNNERS}/{a2}"),
        Some(&a.token),
        "",
    )
    .await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    assert_eq!(fixture.owner(&a2).await, None);
    fixture.cleanup().await;
}

// Value: protects=an operator can manage a tenant's host but never take it
// over; fails_when=the platform scope rotates a tenant token or assigns a
// platform decision model; why_new=§2's tenant-managed refusals; seam=none.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn platform_reach_cannot_rotate_or_edit_a_tenant_runner() {
    let mut fixture = Fixture::create().await;
    let a = fixture
        .caller("Support triage", &[Scope::RunnerTenant])
        .await;
    let operator = fixture
        .caller(
            "Platform operations",
            &[Scope::RunnerRead, Scope::RunnerWrite, Scope::RunnerRegister],
        )
        .await;
    let durable_asked = register(
        &a,
        mint_id().as_str(),
        &body("durable-box", r#""tenant""#, true),
    )
    .await;
    let durable_asked = json_body(durable_asked).await;
    assert_eq!(
        durable_asked
            .pointer("/assigned_policy/durable_tasks_enabled")
            .and_then(Value::as_bool),
        Some(false),
        "a tenant registration stores durable tasks off"
    );
    let runner = Uuid7::parse(text(&durable_asked, "runner_id")).expect("a runner id");
    let digest = fixture.token_hash(&runner).await;
    let admin = format!("/v1/admin/runners/{runner}");
    let operator_patch = |body: &'static str| {
        send(
            &operator.router,
            Method::PATCH,
            &admin,
            Some(&operator.token),
            body,
        )
    };

    let rotate = operator_patch(r#"{"action":"rotate"}"#).await;
    assert_eq!(rotate.status(), StatusCode::CONFLICT);
    assert_eq!(code_of(rotate).await, "UZ-RUN-026");
    let model = format!(
        r#"{{"assigned_policy":{{"sandbox_tier":"dev_none","network_policy":"allow_all","registry_allowlist":[],"worker_count":2,"extra_binds":[],"durable_tasks_enabled":false,"decision_model_id":"{}"}}}}"#,
        mint_id()
    );
    let with_model = send(
        &operator.router,
        Method::PATCH,
        &admin,
        Some(&operator.token),
        &model,
    )
    .await;
    assert_eq!(with_model.status(), StatusCode::BAD_REQUEST);
    assert_eq!(code_of(with_model).await, "UZ-REQ-001");

    assert_eq!(
        operator_patch(r#"{"labels":["relabelled"]}"#)
            .await
            .status(),
        StatusCode::OK
    );
    let durable_on = r#"{"assigned_policy":{"sandbox_tier":"dev_none","network_policy":"allow_all","registry_allowlist":[],"worker_count":2,"extra_binds":[],"durable_tasks_enabled":true}}"#;
    let tenant_on = patch(&a, &runner, durable_on).await;
    assert_eq!(
        tenant_on.status(),
        StatusCode::BAD_REQUEST,
        "only the platform turns durable tasks on"
    );
    assert_eq!(operator_patch(durable_on).await.status(), StatusCode::OK);
    assert_eq!(
        operator_patch(r#"{"action":"cordon"}"#).await.status(),
        StatusCode::OK
    );
    assert_eq!(
        fixture.token_hash(&runner).await,
        digest,
        "no operator call moved the token"
    );
    assert_eq!(
        fixture.owner(&runner).await,
        Some((false, Some(a.tenant.clone())))
    );
    fixture.cleanup().await;
}

// Value: protects=operators see who owns each host; fails_when=a platform row
// carries a null key or a tenant row hides its tenant; why_new=RunnerItem's
// `tenant_id`; seam=none.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn operator_list_names_each_tenant() {
    let mut fixture = Fixture::create().await;
    let a = fixture
        .caller("Support triage", &[Scope::RunnerTenant])
        .await;
    let operator = fixture
        .caller("Platform operations", &[Scope::RunnerRead])
        .await;
    let (tenant_runner, _) = registered(&a, "named-box").await;
    let platform = fixture.platform_runner().await;

    let platform_row = json_body(admin_read(&operator, &platform).await).await;
    assert!(
        platform_row.get("tenant_id").is_none(),
        "absent, never null: {platform_row}"
    );
    let tenant_row = json_body(admin_read(&operator, &tenant_runner).await).await;
    assert_eq!(text(&tenant_row, "tenant_id"), a.tenant.as_str());
    fixture.cleanup().await;
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
