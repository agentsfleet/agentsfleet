//! Real operator HTTP keeps immutable decision revisions and independent prices.

use afd_api_wire::admin::AdminModelCreate;
use axum::Router;
use http::{Method, StatusCode};
use serde_json::{Value, json};

use super::{Fixture, MODEL_CREATE, a_deleted_model_is_gone, json_body, send};

#[path = "../../../afd_runner/tests/integration_runner_plane/decision_fixture.rs"]
mod decision_fixture;

pub(super) async fn lifecycle(router: &Router, fixture: &Fixture) {
    let mut seed = decision_fixture::DecisionModelFixture::create(&fixture.database).await;
    let token = fixture.token.as_str();
    let mut peer = listed(router, token, &seed.model.model_name)
        .await
        .expect("native peer model");
    assert_eq!(
        at(&peer, "/decision/profile"),
        &serde_json::to_value(
            &seed
                .model
                .details
                .decision
                .as_ref()
                .expect("native decision profile")
                .configuration
                .profile
        )
        .expect("complete reviewed profile")
    );
    let body = initial_body(&seed.model);
    refused_creation(router, token, &body).await;
    let current = create(router, token, &body).await;
    let current = revised(router, token, &current).await;
    let mut current = coding(router, token, current).await;
    replace(&mut current, "/decision/enabled", json!(false));
    update(router, token, &current).await;
    a_deleted_model_is_gone(
        router,
        token,
        at(&current, "/id").as_str().expect("created identity"),
    )
    .await;
    assert_eq!(
        listed(
            router,
            token,
            at(&current, "/model_id").as_str().expect("model name")
        )
        .await,
        None
    );
    assert_saved(router, token, &peer).await;
    seed.disable().await;
    replace(&mut peer, "/decision/enabled", json!(false));
    assert_saved(router, token, &peer).await;
    seed.finish().await;
}

fn initial_body(model: &afd_admin::Model) -> Value {
    let mut body: Value = serde_json::from_str(MODEL_CREATE).expect("existing catalogue body");
    let mut price = body.pointer("/prices/0").expect("base price").clone();
    for key in [
        "input_nanos_per_mtok",
        "cached_input_nanos_per_mtok",
        "output_nanos_per_mtok",
    ] {
        put(&mut price, key, json!("42"));
    }
    put(&mut body, "provider", json!("typesafe"));
    put(
        &mut body,
        "model_id",
        json!(format!("admin-decisions-{}", afd_db::test_util::mint_id())),
    );
    put(&mut body, "prices", json!([]));
    put(
        &mut body,
        "decision",
        json!({"enabled":true,
        "profile":model.details.decision.as_ref().expect("reviewed definition").configuration.profile,
        "prices":[price]}),
    );
    canonical(&body)
}

fn canonical(body: &Value) -> Value {
    let text = body.to_string();
    let parsed: AdminModelCreate<'_> = serde_json::from_str(&text).expect("public model body");
    serde_json::to_value(parsed).expect("canonical public instants and optional fields")
}

async fn refused_creation(router: &Router, token: &str, body: &Value) {
    let name = at(body, "/model_id").as_str().expect("unique model name");
    assert_eq!(listed(router, token, name).await, None);
    let mut unsafe_body = body.clone();
    replace(
        &mut unsafe_body,
        "/decision/profile/endpoint",
        json!("http://127.0.0.1/v1/systemone"),
    );
    let problem = request(
        router,
        token,
        Method::POST,
        "/v1/admin/models",
        &unsafe_body,
        StatusCode::BAD_REQUEST,
    )
    .await;
    refused(
        &problem,
        "Decision settings require supported questions, safe HTTPS endpoints, complete thresholds and evaluation evidence",
    );
    assert_eq!(listed(router, token, name).await, None);
}

async fn create(router: &Router, token: &str, body: &Value) -> Value {
    let mut saved = request(
        router,
        token,
        Method::POST,
        "/v1/admin/models",
        body,
        StatusCode::CREATED,
    )
    .await;
    let reference = saved
        .as_object_mut()
        .expect("created response")
        .remove("request_id");
    assert!(
        reference
            .and_then(|value| value.as_str().map(str::to_owned))
            .is_some_and(|value| !value.is_empty())
    );
    let mut expected = body.clone();
    put(&mut expected, "id", at(&saved, "/id").clone());
    assert_eq!(saved, expected);
    assert_saved(router, token, &expected).await;
    expected
}

async fn revised(router: &Router, token: &str, current: &Value) -> Value {
    let mut desired = current.clone();
    replace(
        &mut desired,
        "/decision/profile/goal/statement_min_bps",
        json!(9600),
    );
    let path = path(current);
    let problem = request(
        router,
        token,
        Method::PATCH,
        &path,
        &patch_body(&desired),
        StatusCode::BAD_REQUEST,
    )
    .await;
    refused(
        &problem,
        "Changed decision settings require a new profile revision",
    );
    assert_saved(router, token, current).await;
    replace(
        &mut desired,
        "/decision/profile/revision",
        json!("reviewed-2"),
    );
    update(router, token, &desired).await;
    desired
}

async fn coding(router: &Router, token: &str, mut desired: Value) -> Value {
    let coding: Value = serde_json::from_str(MODEL_CREATE).expect("independent coding schedule");
    let normalized = canonical(&coding);
    put(&mut desired, "prices", at(&normalized, "/prices").clone());
    put(
        &mut desired,
        "inference",
        json!({"api":"chat", "base_url":"https://models.example.com/v1", "max_output_tokens":8192}),
    );
    let mut body = patch_body(&desired);
    put(&mut body, "inference", at(&desired, "/inference").clone());
    let saved = request(
        router,
        token,
        Method::PATCH,
        &path(&desired),
        &body,
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        (at(&saved, "/id").clone(), at(&saved, "/updated").clone()),
        (at(&desired, "/id").clone(), json!(true))
    );
    assert_saved(router, token, &desired).await;
    let mut incomplete = patch_body(&desired);
    put(&mut incomplete, "prices", json!([]));
    let problem = request(
        router,
        token,
        Method::PATCH,
        &path(&desired),
        &incomplete,
        StatusCode::BAD_REQUEST,
    )
    .await;
    refused(
        &problem,
        "Coding models require a nonempty coding price schedule; only decision-only models may omit it",
    );
    assert_saved(router, token, &desired).await;
    desired
}

fn patch_body(model: &Value) -> Value {
    json!({"metadata":at(model, "/metadata"), "prices":at(model, "/prices"), "decision":at(model, "/decision")})
}

fn path(model: &Value) -> String {
    format!(
        "/v1/admin/models/{}",
        at(model, "/id").as_str().expect("stored identity")
    )
}

fn refused(problem: &Value, detail: &str) {
    assert_eq!(
        (
            at(problem, "/error_code").as_str(),
            at(problem, "/detail").as_str()
        ),
        (Some("UZ-REQ-001"), Some(detail))
    );
}

async fn update(router: &Router, token: &str, desired: &Value) {
    let updated = request(
        router,
        token,
        Method::PATCH,
        &path(desired),
        &patch_body(desired),
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        (
            at(&updated, "/id").clone(),
            at(&updated, "/updated").clone()
        ),
        (at(desired, "/id").clone(), json!(true))
    );
    assert_saved(router, token, desired).await;
}

async fn assert_saved(router: &Router, token: &str, expected: &Value) {
    assert_eq!(
        listed(
            router,
            token,
            at(expected, "/model_id")
                .as_str()
                .expect("owned model name")
        )
        .await
        .as_ref(),
        Some(expected)
    );
}

async fn listed(router: &Router, token: &str, name: &str) -> Option<Value> {
    let response = send(router, Method::GET, "/v1/admin/models", Some(token), "").await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let models = at(&body, "/models").as_array().expect("complete catalogue");
    models
        .iter()
        .find(|model| at(model, "/model_id").as_str() == Some(name))
        .cloned()
}

async fn request(
    router: &Router,
    token: &str,
    method: Method,
    path: &str,
    body: &Value,
    status: StatusCode,
) -> Value {
    let response = send(router, method, path, Some(token), &body.to_string()).await;
    assert_eq!(response.status(), status);
    json_body(response).await
}

fn at<'a>(value: &'a Value, path: &str) -> &'a Value {
    value.pointer(path).expect("complete owned model field")
}

fn replace(value: &mut Value, path: &str, replacement: Value) {
    *value
        .pointer_mut(path)
        .expect("complete editable model field") = replacement;
}

fn put(value: &mut Value, field: &str, replacement: Value) {
    let _previous = value
        .as_object_mut()
        .expect("owned model object")
        .insert(field.to_owned(), replacement);
}
