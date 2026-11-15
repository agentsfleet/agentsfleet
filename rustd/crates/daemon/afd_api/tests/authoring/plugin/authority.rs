//! The identity provider's current grant remains authoritative after review.

use super::fixture::{install, make_installable};
use super::{
    Case, Method, StatusCode, Value, header, json, json_body, mint, send, send_with_headers,
};
use afd_auth::{Scope, principal::Subject, scope::ScopeSet};
use axum::Router;

const READER: &[Scope] = &[Scope::FleetRead, Scope::FleetWrite];

async fn revision(
    case: &Case,
    router: &Router,
    plugin: &str,
    tag: &str,
    key: &str,
) -> axum::response::Response {
    send_with_headers(
        router,
        Method::POST,
        &format!("{plugin}/revisions"),
        Some(&case.base.token),
        &json!({"draft_candidate_id":case.candidate}).to_string(),
        &[
            (header::IF_MATCH, tag),
            (header::HeaderName::from_static("idempotency-key"), key),
        ],
    )
    .await
}

async fn publish(case: &Case) -> (String, String, Value) {
    let response = case.publish(mint().as_str()).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let tag = response.headers()[header::ETAG]
        .to_str()
        .expect("tag")
        .to_owned();
    let body = json_body(response).await;
    let path = format!(
        "{}/{}",
        case.collection(),
        body["plugin_id"].as_str().expect("Plugin id")
    );
    (path, tag, body)
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn plugin_permission_preserves_narrowed_authority() {
    let mut case = Case::create().await;
    let (plugin, tag, _) = publish(&case).await;
    let reader = case.base.services(READER).router();
    assert_reader_access(&case, &reader, &plugin).await;
    case.edit().await;
    for (method, path, body) in [
        (
            Method::POST,
            case.collection(),
            json!({"draft_candidate_id":case.candidate,"label":"Review"}).to_string(),
        ),
        (Method::DELETE, plugin.clone(), String::new()),
    ] {
        let denied = send_with_headers(
            &reader,
            method,
            &path,
            Some(&case.base.token),
            &body,
            &[(
                header::HeaderName::from_static("idempotency-key"),
                mint().as_str(),
            )],
        )
        .await;
        assert_scope_denied(denied).await;
    }
    assert_scope_denied(revision(&case, &reader, &plugin, &tag, mint().as_str()).await).await;
    assert_eq!(
        json_body(case.get(&format!("{plugin}/revisions")).await).await["items"]
            .as_array()
            .expect("history")
            .len(),
        1
    );
    case.base.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn plugin_publication_preserves_conflicts_and_revocation() {
    let mut case = Case::create().await;
    make_installable(&mut case).await;
    let (plugin, tag, first) = publish(&case).await;
    let card = json_body(case.get(&plugin).await).await;
    let source =
        json!({"kind":"plugin","plugin_id":first["plugin_id"],"revision_id":first["revision_id"]});
    let (fleet, saved) = install(&case, &source, &card).await;
    case.edit().await;
    let reviewed = json_body(case.get(&case.draft).await).await;
    let services = case.base.services(&Scope::ALL);
    let capabilities = services.capabilities().clone();
    let router = services.router();
    let subject = Subject::new(case.base.owner.as_str()).expect("subject");
    let _revoked = capabilities
        .clone()
        .with(&subject, ScopeSet::from_scopes(READER));
    assert_scope_denied(revision(&case, &router, &plugin, &tag, mint().as_str()).await).await;
    let _restored = capabilities.with(&subject, ScopeSet::from_scopes(&Scope::ALL));
    let stale = revision(&case, &router, &plugin, "\"stale\"", mint().as_str()).await;
    assert_eq!(stale.status(), StatusCode::PRECONDITION_FAILED);
    assert_eq!(json_body(stale).await["error_code"], "UZ-AUTHOR-003");
    let current = json_body(case.get(&plugin).await).await;
    assert_eq!(current["card"]["head_revision_id"], first["revision_id"]);
    assert_eq!(
        json_body(case.get(&fleet).await).await["definition"],
        saved["definition"]
    );
    assert_fresh_review(&case, &router, &reviewed).await;
    assert_publication_replay(&case, &router, &plugin, &tag).await;
    assert_eq!(
        json_body(case.get(&fleet).await).await["definition"],
        saved["definition"]
    );
    case.base.cleanup().await;
}

async fn assert_reader_access(case: &Case, reader: &Router, plugin: &str) {
    for path in [
        case.collection(),
        plugin.to_owned(),
        format!("{plugin}/revisions"),
    ] {
        assert_eq!(
            send(reader, Method::GET, &path, Some(&case.base.token), "")
                .await
                .status(),
            StatusCode::OK
        );
    }
}

async fn assert_scope_denied(response: axum::response::Response) {
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(json_body(response).await["error_code"], "UZ-AUTH-022");
}

async fn assert_publication_replay(case: &Case, router: &Router, plugin: &str, tag: &str) {
    let key = mint();
    let accepted = revision(case, router, plugin, tag, key.as_str()).await;
    assert_eq!(accepted.status(), StatusCode::CREATED);
    let accepted = json_body(accepted).await;
    let replay = revision(case, router, plugin, tag, key.as_str()).await;
    assert_eq!(
        json_body(replay).await["revision_id"],
        accepted["revision_id"]
    );
    assert_eq!(
        json_body(case.get(&format!("{plugin}/revisions")).await).await["items"]
            .as_array()
            .expect("history")
            .len(),
        2
    );
}

async fn assert_fresh_review(case: &Case, router: &Router, reviewed: &Value) {
    let refreshed = send(router, Method::GET, &case.draft, Some(&case.base.token), "").await;
    assert_eq!(refreshed.status(), StatusCode::OK);
    let refreshed = json_body(refreshed).await;
    assert_eq!(refreshed["candidate"], reviewed["candidate"]);
    assert!(refreshed["review"].is_object());
}
