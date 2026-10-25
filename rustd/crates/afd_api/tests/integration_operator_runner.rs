//! Operator runner reads and mutations over the production schema.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use crate::harness;

use afd_auth::credential::Presented;
use afd_auth::directory::Digest;
use afd_auth::scope::{Scope, ScopeSet};
use afd_core::paging::DEFAULT_LIMIT;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{ApiKeySeed, RunnerPolicy, RunnerSeed, TenantSeed, TestDatabase, mint_id};
use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::{Fleet, json_body, send};

#[path = "integration_operator_runner_retirement.rs"]
mod retirement;

pub(crate) const SUBJECT: &str = "user_live_runner_operator";
const NOW: jiff::Timestamp = jiff::Timestamp::constant(1_760_000_000, 0);
const POLICY: &str = r#"{"assigned_policy":{"sandbox_tier":"dev_none","network_policy":"allow_all","registry_allowlist":[],"worker_count":99,"extra_binds":[]}}"#;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn operator_reads_and_mutates_a_real_runner() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let router = Fleet::live(
        fixture.database.clone(),
        SUBJECT,
        ScopeSet::from_scopes(&Scope::ALL),
    )
    .router();
    let runner_path = format!("/v1/fleets/runners/{}", fixture.runner);

    let enrolled_runner = enrol(&router, &fixture.token).await;
    assert_runner_reads(&router, &fixture, &runner_path).await;
    assert_runner_policy_and_actions(&router, &fixture.token, &runner_path).await;
    assert_runner_events(&router, &fixture.token, &runner_path).await;
    assert_terminal_and_missing(&router, &fixture, &runner_path).await;
    retirement::assert_retired(&router, &fixture, &runner_path).await;
    fixture.cleanup(&enrolled_runner).await;
}

async fn assert_runner_reads(router: &axum::Router, fixture: &Fixture, runner_path: &str) {
    // Two reads, because they prove different things and one page cannot prove
    // both. `/v1/fleets/runners` is the PLATFORM list — unlike a fleet's
    // memories or the caller's own workspaces, nothing about the path scopes it
    // to this fixture. A `limit=1` page therefore holds whichever runner sorts
    // first among every runner every concurrent test has seeded, which is not
    // this one: `enrol` has already added another. Asking a one-item page to
    // contain a specific id is a race, and it lost.
    //
    // Dropping `limit` does not settle that race, it only raises its threshold.
    // The unqualified list answers at most `DEFAULT_LIMIT` rows ordered
    // `created_at DESC, id DESC` (`afd_runner/src/sql/runner_view.rs:18`), and
    // the harness clock is frozen — a full lane leaves 89 rows across 10
    // distinct `created_at` values, so the sort collapses onto `id DESC` and
    // which rows reach page one is decided by uuid. Asking that page to hold a
    // specific id is the same coin flip with a bigger coin.
    //
    // So the page proves what a page can: the list answers, it is bounded, and
    // its `total` counts rows the page itself does not carry. This runner's
    // identity is proved by the detail read below, which names it in the path.
    let page = send(
        router,
        Method::GET,
        "/v1/fleets/runners?limit=1",
        Some(&fixture.token),
        "",
    )
    .await;
    assert_eq!(page.status(), StatusCode::OK);
    let page = json_body(page).await;
    assert_eq!(
        page.get("items").and_then(Value::as_array).map(Vec::len),
        Some(1),
        "limit bounds the page"
    );

    let listed = send(
        router,
        Method::GET,
        "/v1/fleets/runners",
        Some(&fixture.token),
        "",
    )
    .await;
    assert_eq!(listed.status(), StatusCode::OK);
    let listed = json_body(listed).await;
    let items = listed
        .get("items")
        .and_then(Value::as_array)
        .expect("the platform list answers an items array");
    assert!(
        !items.is_empty(),
        "the platform list carries rows while this fixture's runner exists"
    );
    assert!(
        u32::try_from(items.len()).is_ok_and(|len| len <= DEFAULT_LIMIT),
        "an unqualified page is bounded by the default limit"
    );
    assert!(
        listed
            .get("total")
            .and_then(Value::as_i64)
            .is_some_and(|total| usize::try_from(total).is_ok_and(|total| total >= items.len())),
        "total counts every runner, not only the ones on this page"
    );

    let detail = send(router, Method::GET, runner_path, Some(&fixture.token), "").await;
    assert_eq!(detail.status(), StatusCode::OK);
    assert_eq!(text(&json_body(detail).await, "id"), fixture.runner);

    // A runner that has held no lease answers an empty page of its history,
    // not a refusal: the read is scoped by the path, so the page is this
    // runner's alone.
    let leases = send(
        router,
        Method::GET,
        &format!("{runner_path}/leases"),
        Some(&fixture.token),
        "",
    )
    .await;
    assert_eq!(leases.status(), StatusCode::OK);
    let leases = json_body(leases).await;
    assert_eq!(
        leases.get("items").and_then(Value::as_array).map(Vec::len),
        Some(0),
        "{leases}"
    );
}

async fn assert_runner_policy_and_actions(router: &axum::Router, token: &str, path: &str) {
    let policy = patch(router, token, path, POLICY).await;
    assert_eq!(policy.status(), StatusCode::OK);
    assert_eq!(
        json_body(policy)
            .await
            .pointer("/assigned_policy/worker_count")
            .and_then(Value::as_u64),
        Some(64),
        "the shared policy bound is reflected on the wire"
    );

    let selftest = patch(router, token, path, r#"{"action":"self_test"}"#).await;
    assert_eq!(selftest.status(), StatusCode::OK);
    assert!(
        json_body(selftest)
            .await
            .get("selftest_requested_at")
            .and_then(Value::as_str)
            .is_some_and(|at| at.parse::<jiff::Timestamp>().is_ok())
    );

    let rotated = patch(router, token, path, r#"{"action":"rotate"}"#).await;
    assert_eq!(rotated.status(), StatusCode::OK);
    assert_eq!(
        rotated.headers().get(http::header::CACHE_CONTROL),
        Some(&http::HeaderValue::from_static("no-store"))
    );
    assert!(text(&json_body(rotated).await, "runner_token").starts_with("agt_r"));

    for action in ["cordon", "cordon", "revoke"] {
        let response = patch(router, token, path, &format!(r#"{{"action":"{action}"}}"#)).await;
        assert_eq!(response.status(), StatusCode::OK, "action={action}");
    }
}

async fn assert_terminal_and_missing(router: &axum::Router, fixture: &Fixture, runner_path: &str) {
    let terminal = patch(router, &fixture.token, runner_path, POLICY).await;
    assert_eq!(terminal.status(), StatusCode::BAD_REQUEST);

    let missing = send(
        router,
        Method::GET,
        &format!("/v1/fleets/runners/{}", mint_id()),
        Some(&fixture.token),
        "",
    )
    .await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
}

async fn enrol(router: &axum::Router, token: &str) -> String {
    let enrolled = send(
        router,
        Method::POST,
        "/v1/runners",
        Some(token),
        r#"{
          "host_id":"operator-live-enrolment",
          "assigned_policy":{
            "sandbox_tier":"dev_none",
            "network_policy":"allow_all",
            "registry_allowlist":[],
            "worker_count":99,
            "extra_binds":[]
          },
          "labels":["live-http"]
        }"#,
    )
    .await;
    assert_eq!(enrolled.status(), StatusCode::CREATED);
    let enrolled = json_body(enrolled).await;
    let enrolled_runner = text(&enrolled, "runner_id").to_owned();
    assert!(text(&enrolled, "runner_token").starts_with("agt_r"));
    assert_eq!(
        enrolled
            .pointer("/assigned_policy/worker_count")
            .and_then(Value::as_u64),
        Some(64),
        "the HTTP reply carries the clamped policy that was stored"
    );
    enrolled_runner
}

async fn assert_runner_events(router: &axum::Router, token: &str, runner_path: &str) {
    let events = send(
        router,
        Method::GET,
        &format!("{runner_path}/events?limit=2&event_type=runner_revoked,runner_cordoned"),
        Some(token),
        "",
    )
    .await;
    assert_eq!(events.status(), StatusCode::OK);
    let events = json_body(events).await;
    assert!(
        events
            .get("items")
            .and_then(Value::as_array)
            .is_some_and(|items| !items.is_empty()),
        "the operator history reads mutations written through the same HTTP surface"
    );
    assert!(
        events
            .get("total")
            .and_then(Value::as_u64)
            .is_some_and(|total| total >= 2),
        "the history reports the complete filtered count"
    );

    for path in [
        "/v1/fleets/runners/not-a-runner/events".to_owned(),
        format!("{runner_path}/events?limit=0"),
    ] {
        let malformed = send(router, Method::GET, &path, Some(token), "").await;
        assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
    }
}

pub(crate) async fn patch(
    router: &axum::Router,
    token: &str,
    path: &str,
    body: &str,
) -> axum::response::Response {
    send(router, Method::PATCH, path, Some(token), body).await
}

pub(crate) fn text<'value>(document: &'value Value, field: &str) -> &'value str {
    document
        .get(field)
        .and_then(Value::as_str)
        .expect("the response field is a string")
}

/// One tenant credential holding every scope, and one seeded runner — shared
/// with the label replacement's live suite, which needs exactly this.
pub(crate) struct Fixture {
    lane: TestDatabase,
    pub(crate) database: Db,
    tenant: String,
    key: String,
    pub(crate) runner: String,
    pub(crate) token: String,
}

impl Fixture {
    pub(crate) async fn create() -> Self {
        let lane = TestDatabase::shared();
        let token_bits = format!("{}{}", mint_id(), mint_id()).replace('-', "");
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            tenant: mint_id(),
            key: mint_id(),
            runner: mint_id(),
            token: format!("agt_t{token_bits}"),
            lane,
        }
    }

    pub(crate) async fn seed(&self) {
        let digest = Digest::of(&Presented::new(&self.token).expect("the token is valid"));
        let mut connection = self.database.acquire().await.expect("an API connection");
        TenantSeed::new(&self.tenant)
            .name("Runner operator")
            .at(NOW)
            .insert(&mut *connection)
            .await
            .expect("the operator's tenant seeds");
        ApiKeySeed::new(&self.key, &self.tenant, digest.as_str(), SUBJECT)
            .at(NOW)
            .insert(&mut *connection)
            .await
            .expect("the operator's key seeds");
        RunnerSeed::new(
            &self.runner,
            afd_state::sql::RUNNER_STATUS_ACTIVE,
            RunnerPolicy {
                sandbox_tier: "dev_none",
                network_policy: "deny_all_egress",
            },
        )
        .labels(r#"["linux"]"#)
        .seen(NOW)
        .at(NOW)
        .insert(&mut *connection)
        .await
        .expect("the operator and runner seed");
    }

    pub(crate) async fn cleanup(self, enrolled_runner: &str) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query("DELETE FROM runner.runners WHERE id = ANY($1::uuid[])")
            .bind(vec![self.runner.as_str(), enrolled_runner])
            .execute(&mut *connection)
            .await
            .expect("the scoped runners clean up");
        sqlx::query("DELETE FROM identity.tenants WHERE id = $1::uuid")
            .bind(&self.tenant)
            .execute(&mut *connection)
            .await
            .expect("the scoped tenant cleans up");
        drop(connection);
        drop(self.database);
        self.lane.cleanup().await;
    }
}
