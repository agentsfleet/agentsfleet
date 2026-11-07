//! The served routes expose the same revision and replay rules as the domain.

use super::{
    Case, Method, Response, StatusCode, Value, header, json, json_body, mint, send,
    send_with_headers,
};

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn continued_chat_publishes_explicit_v2_and_retains_v1_and_its_original_replay() {
    let mut case = Case::create().await;
    let (library, tag, first) = publish_v1(&case).await;
    let v1 = json_body(case.get(&library).await).await;
    case.edit().await;
    let revision_key = mint();
    let body = json!({"draft_candidate_id":case.candidate}).to_string();
    let headers = [
        (header::IF_MATCH, tag.as_str()),
        (
            header::HeaderName::from_static("idempotency-key"),
            revision_key.as_str(),
        ),
    ];
    let second = post_revision(&case, &library, &body, &headers).await;
    assert_eq!(second.status(), StatusCode::CREATED);
    let revision = header_text(&second, header::LOCATION, "revision location");
    let second = json_body(second).await;
    assert_eq!(first["library_id"], second["library_id"]);
    assert_ne!(first["revision_id"], second["revision_id"]);
    assert_v1_stays_pinned_beside_v2(&case, &library, &revision, &first, &v1).await;
    let replay = post_revision(&case, &library, &body, &headers).await;
    assert_eq!(replay.status(), StatusCode::CREATED);
    assert_eq!(
        json_body(replay).await["revision_id"],
        second["revision_id"]
    );
    assert_history_then_deletion_hides_both(&case, &library, &revision).await;
    case.base.cleanup().await;
}

/// Publishes the first revision and answers its library location, its tag and
/// its body — after proving the draft stayed editable behind it.
async fn publish_v1(case: &Case) -> (String, String, Value) {
    let first = case.publish(mint().as_str()).await;
    assert_eq!(first.status(), StatusCode::CREATED);
    let library = header_text(&first, header::LOCATION, "Library location");
    let tag = header_text(&first, header::ETAG, "Library tag");
    let first = json_body(first).await;
    assert_eq!(
        json_body(case.get(&case.draft).await).await["state"],
        "editing"
    );
    (library, tag, first)
}

/// A header the route must have set, owned so the response can be consumed.
fn header_text(response: &Response, name: header::HeaderName, what: &str) -> String {
    response.headers()[name].to_str().expect(what).to_owned()
}

/// One `POST {library}/revisions` with the caller's body and headers.
async fn post_revision(
    case: &Case,
    library: &str,
    body: &str,
    headers: &[(header::HeaderName, &str)],
) -> Response {
    send_with_headers(
        &case.router,
        Method::POST,
        &format!("{library}/revisions"),
        Some(&case.base.token),
        body,
        headers,
    )
    .await
}

/// V1 still reads back under its own revision id, and the new revision is `2`.
async fn assert_v1_stays_pinned_beside_v2(
    case: &Case,
    library: &str,
    revision: &str,
    first: &Value,
    v1: &Value,
) {
    let pinned = case
        .get(&format!(
            "{library}?revision_id={}",
            first["revision_id"].as_str().expect("V1")
        ))
        .await;
    assert_eq!(
        json_body(pinned).await["head"]["definition"],
        v1["head"]["definition"]
    );
    assert_eq!(
        json_body(case.get(revision).await).await["revision"]["revision"],
        "2"
    );
}

/// History lists both revisions without definitions; deleting the library
/// then hides the library and every revision under it.
async fn assert_history_then_deletion_hides_both(case: &Case, library: &str, revision: &str) {
    let history = json_body(case.get(&format!("{library}/revisions")).await).await;
    assert_eq!(history["items"].as_array().expect("history").len(), 2);
    assert!(history["items"][0].get("definition").is_none());
    let removed = send(
        &case.router,
        Method::DELETE,
        library,
        Some(&case.base.token),
        "",
    )
    .await;
    assert_eq!(removed.status(), StatusCode::NO_CONTENT);
    assert_eq!(case.get(library).await.status(), StatusCode::NOT_FOUND);
    assert_eq!(case.get(revision).await.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn stale_library_tag_returns_the_current_tag_without_appending() {
    let mut case = Case::create().await;
    let first = case.publish(mint().as_str()).await;
    let library = first.headers()[header::LOCATION]
        .to_str()
        .expect("location")
        .to_owned();
    let tag = first.headers()[header::ETAG].clone();
    case.edit().await;
    let response = send_with_headers(
        &case.router,
        Method::POST,
        &format!("{library}/revisions"),
        Some(&case.base.token),
        &json!({"draft_candidate_id":case.candidate}).to_string(),
        &[
            (header::IF_MATCH, "\"stale\""),
            (
                header::HeaderName::from_static("idempotency-key"),
                mint().as_str(),
            ),
        ],
    )
    .await;
    assert_eq!(response.status(), StatusCode::PRECONDITION_FAILED);
    assert_eq!(
        json_body(response).await["etag"].as_str(),
        tag.to_str().ok()
    );
    assert_eq!(
        json_body(case.get(&format!("{library}/revisions")).await).await["items"]
            .as_array()
            .expect("history")
            .len(),
        1
    );
    case.base.cleanup().await;
}
