//! Plugin fixtures build installable revisions through the native router.

use super::{Case, Method, StatusCode, Value, header, json, json_body, mint, send_with_headers};

/// The installed fleet's name, which is also the card's recipe name.
const NAME: &str = "card-install";

/// Gives the source draft a runtime model and the recipe name, so a fleet can
/// be installed from what it publishes.
pub(super) async fn make_installable(case: &mut Case) {
    let current = case.get(&case.draft).await;
    let tag = current.headers()[header::ETAG]
        .to_str()
        .expect("draft tag")
        .to_owned();
    let patch = json!({"runtime_model_ref":case.base.model,"config_ops":[
        {"op":"replace","path":"/config/name","value":NAME}
    ]});
    let response = send_with_headers(
        &case.router,
        Method::PATCH,
        &case.draft,
        Some(&case.base.token),
        &patch.to_string(),
        &[(header::IF_MATCH, &tag)],
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    case.candidate = json_body(response).await["candidate"]["id"].clone();
}

/// Installs a fleet from the card's revision; answers its location and body.
pub(super) async fn install(case: &Case, source: &Value, card: &Value) -> (String, Value) {
    let body = json!({"source":source,"name":NAME,
        "review_digest":card["review"]["review_digest"],"start":{"kind":"none"}});
    let response = send_with_headers(
        &case.router,
        Method::POST,
        &format!("/v1/workspaces/{}/fleets", case.base.workspace),
        Some(&case.base.token),
        &body.to_string(),
        &[(
            header::HeaderName::from_static("idempotency-key"),
            mint().as_str(),
        )],
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let path = response.headers()[header::LOCATION]
        .to_str()
        .expect("fleet location")
        .to_owned();
    (path, json_body(response).await)
}
