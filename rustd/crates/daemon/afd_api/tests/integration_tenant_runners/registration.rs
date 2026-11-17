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
    let other = json_body(other).await;
    assert_eq!(
        other.get("error_code").and_then(Value::as_str),
        Some("UZ-RUN-025"),
        "{other}"
    );
    assert_eq!(
        other.get("current_state").and_then(Value::as_str),
        Some("registered"),
        "{other}"
    );

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
    assert_eq!(code_of(not_v7).await, "UZ-REQ-001");

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
            // Any other answer leaves the totals below short of every caller.
            _ => {}
        }
        runners.insert(runner);
    }
    assert_eq!((created, replayed), (1, CALLERS - 1));
    assert_eq!(runners.len(), 1, "every answer names the same runner");
    assert_eq!(fixture.tenant_runners(&owner.tenant).await, 1);
    fixture.cleanup().await;
}

// Value: protects=a key keeps answering after its runner is gone, and never
// registers a second one; fails_when=the receipt goes with the runner or a
// retry reinserts; why_new=the receipt outlives its runner; seam=none.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_retry_after_deletion_registers_nothing() {
    let mut fixture = Fixture::create().await;
    let owner = fixture
        .caller("Support triage", &[Scope::RunnerTenant])
        .await;
    let key = mint_id();
    let request = body("short-lived-box", r#""tenant""#, false);
    let first = json_body(register(&owner, key.as_str(), &request).await).await;
    let runner = text(&first, "runner_id").to_owned();
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

    let retried = register(&owner, key.as_str(), &request).await;
    assert_eq!(
        retried.status(),
        StatusCode::OK,
        "the key replays, it does not register again"
    );
    assert_eq!(text(&json_body(retried).await, "runner_id"), runner);
    assert_eq!(fixture.tenant_runners(&owner.tenant).await, 0);
    fixture.cleanup().await;
}
