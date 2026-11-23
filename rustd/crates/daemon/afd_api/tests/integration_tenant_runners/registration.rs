use super::*;

// Value: protects=a tenant's registration is owned by that tenant, and a
// platform registration by the platform; fails_when=the row's owner is never
// set, or set from a missing tenant; why_new=the tenant route; seam=none.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn tenant_registers_a_tenant_runner() {
    let mut fixture = Fixture::create().await;
    let owner = fixture
        .caller("Support triage", &[Scope::RunnerTenant])
        .await;

    let created = register(&owner, &body("build-box-1", r#""tenant""#, false)).await;
    assert_eq!(created.status(), StatusCode::CREATED);
    assert_eq!(
        created
            .headers()
            .get(http::header::CACHE_CONTROL)
            .map(http::HeaderValue::as_bytes),
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

    let refused = register(&signup, &body("refused-box", r#""tenant""#, false)).await;
    assert_eq!(refused.status(), StatusCode::FORBIDDEN);
    assert_eq!(code_of(refused).await, "UZ-AUTH-022");
    assert_eq!(fixture.tenant_runners(&signup.tenant).await, 0);

    let owner = fixture
        .caller("Support triage", &[Scope::RunnerTenant])
        .await;
    let (_, runner_token) = registered(&owner, "runner-box").await;
    let as_runner = send(
        &owner.router,
        Method::POST,
        TENANT_RUNNERS,
        Some(&runner_token),
        &body("from-a-runner", r#""tenant""#, false),
    )
    .await;
    assert_eq!(as_runner.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(fixture.tenant_runners(&owner.tenant).await, 1);
    fixture.cleanup().await;
}

// Value: protects=a host registers once, and its tenant recovers the token by
// rotation; fails_when=a retry makes a second runner, the 409 names no runner,
// or the rotated token does not connect; why_new=the natural key replacing the registration receipt; seam=none.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_retried_registration_of_one_host_answers_409_naming_the_runner() {
    let mut fixture = Fixture::create().await;
    let owner = fixture
        .caller("Support triage", &[Scope::RunnerTenant])
        .await;
    let (runner, first_token) = registered(&owner, "retry-box").await;

    let retried = register(&owner, &body("retry-box", r#""other""#, false)).await;
    assert_eq!(retried.status(), StatusCode::CONFLICT);
    assert_eq!(
        retried
            .headers()
            .get(http::header::LOCATION)
            .and_then(|value| value.to_str().ok()),
        Some(format!("{TENANT_RUNNERS}/{runner}").as_str()),
        "the refusal names the runner the host already is"
    );
    let retried = json_body(retried).await;
    assert_eq!(
        retried.get("error_code").and_then(Value::as_str),
        Some("UZ-RUN-033"),
        "{retried}"
    );
    assert_eq!(
        retried.get("current_state").and_then(Value::as_str),
        Some("registered"),
        "{retried}"
    );
    assert!(
        retried.get("runner_token").is_none(),
        "a refusal never carries a token: {retried}"
    );
    assert_eq!(fixture.tenant_runners(&owner.tenant).await, 1);

    let rotated = patch(&owner, &runner, r#"{"action":"rotate"}"#).await;
    assert_eq!(rotated.status(), StatusCode::OK);
    let token = text(&json_body(rotated).await, "runner_token").to_owned();
    let connects = send(
        &owner.router,
        Method::GET,
        "/v1/runners/me",
        Some(&token),
        "",
    )
    .await;
    assert_eq!(
        connects.status(),
        StatusCode::OK,
        "the rotated token connects"
    );
    let retired = send(
        &owner.router,
        Method::GET,
        "/v1/runners/me",
        Some(&first_token),
        "",
    )
    .await;
    assert_eq!(retired.status(), StatusCode::UNAUTHORIZED);
    fixture.cleanup().await;
}

// Value: protects=the host is unique within each tenant, not across them, and the route
// honours no `Idempotency-Key`; fails_when=the constraint spans tenants, or a
// key replays an earlier answer; why_new=the natural key; seam=none.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn one_host_registers_once_in_each_tenant_whatever_key_it_sends() {
    let mut fixture = Fixture::create().await;
    let a = fixture
        .caller("Support triage", &[Scope::RunnerTenant])
        .await;
    let b = fixture
        .caller("Pull request review", &[Scope::RunnerTenant])
        .await;
    let key = mint_id();

    assert_eq!(
        keyed(&a, "shared-box", &key).await.status(),
        StatusCode::CREATED
    );
    assert_eq!(
        keyed(&b, "shared-box", &key).await.status(),
        StatusCode::CREATED,
        "another tenant registers the same host_id"
    );
    assert_eq!(
        keyed(&a, "second-box", &key).await.status(),
        StatusCode::CREATED,
        "the key is not honoured, so another host registers under it"
    );
    assert_eq!(fixture.tenant_runners(&a.tenant).await, 2);
    assert_eq!(fixture.tenant_runners(&b.tenant).await, 1);

    let model = format!(
        r#"{{"host_id":"model-box","assigned_policy":{{"sandbox_tier":"dev_none","network_policy":"allow_all","registry_allowlist":[],"worker_count":2,"extra_binds":[],"durable_tasks_enabled":false,"decision_model_id":"{}"}},"labels":[]}}"#,
        mint_id()
    );
    let with_model = register(&a, &model).await;
    assert_eq!(with_model.status(), StatusCode::BAD_REQUEST);
    assert_eq!(code_of(with_model).await, "UZ-REQ-001");
    assert_eq!(fixture.tenant_runners(&a.tenant).await, 2);
    fixture.cleanup().await;
}

/// A registration of `host` that also sends `key`, which the route ignores.
async fn keyed(caller: &Caller, host: &str, key: &Uuid7) -> http::Response<axum::body::Body> {
    send_with_headers(
        &caller.router,
        Method::POST,
        TENANT_RUNNERS,
        Some(&caller.token),
        &body(host, r#""tenant""#, false),
        &[(IDEMPOTENCY_KEY, key.as_str())],
    )
    .await
}

// Value: protects=concurrent registrations of one host leave one runner, and
// every loser is told which; fails_when=the unique constraint is missing or the
// conflict read runs before the winner commits; why_new=the natural key;
// seam=none (TEST-CONTEND).
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn concurrent_registrations_of_one_host_leave_one_runner() {
    const CALLERS: usize = 100;
    let mut fixture = Fixture::create().await;
    let owner = Arc::new(
        fixture
            .caller("Support triage", &[Scope::RunnerTenant])
            .await,
    );
    let barrier = Arc::new(Barrier::new(CALLERS));
    let mut tasks = Vec::with_capacity(CALLERS);
    for _ in 0..CALLERS {
        let (owner, barrier) = (Arc::clone(&owner), Arc::clone(&barrier));
        tasks.push(tokio::spawn(async move {
            barrier.wait().await;
            let answer = register(&owner, &body("contended-box", r#""tenant""#, false)).await;
            let status = answer.status();
            let location = answer
                .headers()
                .get(http::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            let runner = match location {
                Some(location) => location.rsplit('/').next().map(str::to_owned),
                None => json_body(answer)
                    .await
                    .get("runner_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            };
            (status, runner)
        }));
    }
    let mut created = 0;
    let mut refused = 0;
    let mut runners = std::collections::BTreeSet::new();
    for task in tasks {
        let (status, runner) = task.await.expect("the registration task finishes");
        match status {
            StatusCode::CREATED => created += 1,
            StatusCode::CONFLICT => refused += 1,
            // Any other answer leaves the totals below short of every caller.
            _ => {}
        }
        runners.insert(runner);
    }
    assert_eq!((created, refused), (1, CALLERS - 1));
    assert_eq!(runners.len(), 1, "every answer names the same runner");
    assert!(
        runners.iter().all(Option::is_some),
        "every answer names a runner"
    );
    assert_eq!(fixture.tenant_runners(&owner.tenant).await, 1);
    fixture.cleanup().await;
}

// Value: protects=a deleted runner frees its host; fails_when=the host stays
// taken after its row is gone; why_new=the natural key outlives no runner;
// seam=none.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_deleted_runners_host_registers_again() {
    let mut fixture = Fixture::create().await;
    let owner = fixture
        .caller("Support triage", &[Scope::RunnerTenant])
        .await;
    let (runner, _) = registered(&owner, "short-lived-box").await;
    let path = format!("{TENANT_RUNNERS}/{runner}");
    let revoked = send(
        &owner.router,
        Method::PATCH,
        &path,
        Some(&owner.token),
        r#"{"action":"revoke"}"#,
    )
    .await;
    assert_eq!(revoked.status(), StatusCode::OK);
    let deleted = send(&owner.router, Method::DELETE, &path, Some(&owner.token), "").await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);

    let (again, _) = registered(&owner, "short-lived-box").await;
    assert_ne!(again, runner, "the host registers as a new runner");
    assert_eq!(fixture.tenant_runners(&owner.tenant).await, 1);
    fixture.cleanup().await;
}
