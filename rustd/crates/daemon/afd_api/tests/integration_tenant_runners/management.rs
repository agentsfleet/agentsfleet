use super::*;

/// A policy body a tenant may send: durable tasks off, `workers` workers.
fn tenant_policy(workers: u32) -> String {
    format!(
        r#"{{"assigned_policy":{{"sandbox_tier":"dev_none","network_policy":"allow_all","registry_allowlist":[],"worker_count":{workers},"extra_binds":[],"durable_tasks_enabled":false}}}}"#
    )
}

// Value: protects=a tenant's list holds exactly its own runners; fails_when=the
// list or its count binds no owner, or one that matches every row;
// why_new=RunnerScope; seam=none.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn tenant_lists_only_its_runners() {
    let mut fixture = Fixture::create().await;
    let a = fixture
        .caller("Support triage", &[Scope::RunnerTenant])
        .await;
    let b = fixture
        .caller("Pull request review", &[Scope::RunnerTenant])
        .await;
    let (a1, _) = registered(&a, "a-one").await;
    let (a2, _) = registered(&a, "a-two").await;
    registered(&b, "b-one").await;
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
    let first_page = format!("{TENANT_RUNNERS}?limit=1");
    let page = json_body(send(&a.router, Method::GET, &first_page, Some(&a.token), "").await).await;
    assert_eq!(page["items"].as_array().map(Vec::len), Some(1));
    fixture.cleanup().await;
}

// Value: protects=a tenant manages its own runner through every verb;
// fails_when=a tenant scope refuses its own row, a policy edit is refused, or a
// rotation leaves the old token live; why_new=RunnerScope; seam=none.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn tenant_manages_its_own_runner() {
    let mut fixture = Fixture::create().await;
    let a = fixture
        .caller("Support triage", &[Scope::RunnerTenant])
        .await;
    let (a1, _) = registered(&a, "a-one").await;
    let (a2, _) = registered(&a, "a-two").await;

    let own = format!("{TENANT_RUNNERS}/{a1}");
    let read = send(&a.router, Method::GET, &own, Some(&a.token), "").await;
    assert_eq!(read.status(), StatusCode::OK);
    for edit in [
        r#"{"action":"cordon"}"#.to_owned(),
        r#"{"labels":["tenant","gpu"]}"#.to_owned(),
        tenant_policy(3),
        r#"{"action":"self_test"}"#.to_owned(),
    ] {
        assert_eq!(
            patch(&a, &a1, &edit).await.status(),
            StatusCode::OK,
            "{edit}"
        );
    }
    assert_eq!(
        fixture.worker_count(&a1).await,
        3,
        "the tenant's policy edit landed"
    );

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

    for action in [r#"{"action":"drain"}"#, r#"{"action":"revoke"}"#] {
        assert_eq!(
            patch(&a, &a2, action).await.status(),
            StatusCode::OK,
            "{action}"
        );
    }
    let path = format!("{TENANT_RUNNERS}/{a2}");
    let deleted = send(&a.router, Method::DELETE, &path, Some(&a.token), "").await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    assert_eq!(fixture.owner(&a2).await, None);
    fixture.cleanup().await;
}

// Value: protects=a tenant never reads or changes another tenant's runner;
// fails_when=any scoped statement drops the owner; why_new=RunnerScope;
// seam=none.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn tenant_never_reaches_another_tenants_runner() {
    let mut fixture = Fixture::create().await;
    let a = fixture
        .caller("Support triage", &[Scope::RunnerTenant])
        .await;
    let b = fixture
        .caller("Pull request review", &[Scope::RunnerTenant])
        .await;
    let (b1, b_token) = registered(&b, "b-one").await;
    let b_digest = fixture.token_hash(&b1).await;

    let foreign = format!("{TENANT_RUNNERS}/{b1}");
    let refusals = [
        send(&a.router, Method::GET, &foreign, Some(&a.token), "").await,
        patch(&a, &b1, r#"{"action":"cordon"}"#).await,
        patch(&a, &b1, r#"{"action":"drain"}"#).await,
        patch(&a, &b1, r#"{"action":"revoke"}"#).await,
        patch(&a, &b1, r#"{"labels":["taken"]}"#).await,
        patch(&a, &b1, &tenant_policy(5)).await,
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
    let alive = send(&b.router, Method::GET, "/v1/runners/me", Some(&b_token), "").await;
    assert_eq!(
        alive.status(),
        StatusCode::OK,
        "and B's runner is still in service"
    );
    fixture.cleanup().await;
}

/// A full policy body with `durable` durable tasks.
fn policy(durable: bool) -> String {
    format!(
        r#"{{"assigned_policy":{{"sandbox_tier":"dev_none","network_policy":"allow_all","registry_allowlist":[],"worker_count":2,"extra_binds":[],"durable_tasks_enabled":{durable}}}}}"#
    )
}

/// An operator's PATCH of `runner` through the admin route.
async fn operator_patch(
    operator: &Caller,
    runner: &Uuid7,
    body: &str,
) -> http::Response<axum::body::Body> {
    let path = format!("/v1/admin/runners/{runner}");
    send(
        &operator.router,
        Method::PATCH,
        &path,
        Some(&operator.token),
        body,
    )
    .await
}

async fn operator(fixture: &mut Fixture) -> Caller {
    let scopes = [Scope::RunnerRead, Scope::RunnerWrite, Scope::RunnerRegister];
    fixture.caller("Platform operations", &scopes).await
}

// Value: protects=an operator can manage a tenant's host but never take it
// over; fails_when=the platform scope rotates a tenant token or assigns a
// platform decision model; why_new=the tenant-managed refusals; seam=none.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn platform_reach_cannot_rotate_or_edit_a_tenant_runner() {
    let mut fixture = Fixture::create().await;
    let a = fixture
        .caller("Support triage", &[Scope::RunnerTenant])
        .await;
    let operator = operator(&mut fixture).await;
    let (runner, _) = registered(&a, "operated-box").await;
    let digest = fixture.token_hash(&runner).await;

    let rotate =
        json_body(operator_patch(&operator, &runner, r#"{"action":"rotate"}"#).await).await;
    assert_eq!(
        rotate["error_code"].as_str(),
        Some("UZ-RUN-026"),
        "{rotate}"
    );
    assert_eq!(
        rotate["current_state"].as_str(),
        Some("tenant_managed"),
        "{rotate}"
    );
    let model = format!(
        r#"{{"assigned_policy":{{"sandbox_tier":"dev_none","network_policy":"allow_all","registry_allowlist":[],"worker_count":2,"extra_binds":[],"durable_tasks_enabled":false,"decision_model_id":"{}"}}}}"#,
        mint_id()
    );
    let with_model = operator_patch(&operator, &runner, &model).await;
    assert_eq!(with_model.status(), StatusCode::BAD_REQUEST);
    assert_eq!(code_of(with_model).await, "UZ-REQ-001");

    for edit in [r#"{"labels":["relabelled"]}"#, r#"{"action":"cordon"}"#] {
        assert_eq!(
            operator_patch(&operator, &runner, edit).await.status(),
            StatusCode::OK,
            "{edit}"
        );
    }
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

// Value: protects=only a platform admin turns a tenant runner's durable tasks
// on, and a tenant keeps them on through later edits; fails_when=registration
// stores the body's flag, a tenant turns them on, or a tenant's edit is refused
// for keeping them; why_new=the durable-task rule; seam=none.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn only_the_platform_turns_durable_tasks_on() {
    let mut fixture = Fixture::create().await;
    let a = fixture
        .caller("Support triage", &[Scope::RunnerTenant])
        .await;
    let operator = operator(&mut fixture).await;
    let asked = json_body(
        register(
            &a,
            mint_id().as_str(),
            &body("durable-box", r#""tenant""#, true),
        )
        .await,
    )
    .await;
    let runner = Uuid7::parse(text(&asked, "runner_id")).expect("a runner id");
    assert_eq!(
        asked
            .pointer("/assigned_policy/durable_tasks_enabled")
            .and_then(Value::as_bool),
        Some(false),
        "the answer says off"
    );
    assert!(
        !fixture.durable_tasks(&runner).await,
        "and the row stores off"
    );

    let tenant_on = patch(&a, &runner, &policy(true)).await;
    assert_eq!(
        tenant_on.status(),
        StatusCode::BAD_REQUEST,
        "a tenant cannot turn them on"
    );
    assert_eq!(
        operator_patch(&operator, &runner, &policy(true))
            .await
            .status(),
        StatusCode::OK
    );
    assert!(
        fixture.durable_tasks(&runner).await,
        "the platform admin turned them on"
    );
    assert_eq!(
        patch(&a, &runner, &policy(true)).await.status(),
        StatusCode::OK,
        "the tenant may keep them"
    );
    assert_eq!(
        patch(&a, &runner, &policy(false)).await.status(),
        StatusCode::OK,
        "and turn them off"
    );
    assert!(!fixture.durable_tasks(&runner).await);
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
