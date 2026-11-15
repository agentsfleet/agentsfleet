//! A collection card names its head's ordinal from the same revision row.

use super::{
    Case, Method, StatusCode, Value, header, json, json_body, mint, send, send_with_headers,
};

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn plugin_card_ordinal_matches_pinned_head() {
    let mut case = Case::create().await;
    let first = case.publish(mint().as_str()).await;
    assert_eq!(first.status(), StatusCode::CREATED);
    let tag = first.headers()[header::ETAG]
        .to_str()
        .expect("tag")
        .to_owned();
    let first = json_body(first).await;
    let plugin = format!(
        "{}/{}",
        case.collection(),
        first["plugin_id"].as_str().expect("Plugin id")
    );
    assert_listed_head(&case, &first, "1").await;
    case.edit().await;
    let second = send_with_headers(
        &case.router,
        Method::POST,
        &format!("{plugin}/revisions"),
        Some(&case.base.token),
        &json!({"draft_candidate_id":case.candidate}).to_string(),
        &[
            (header::IF_MATCH, tag.as_str()),
            (
                header::HeaderName::from_static("idempotency-key"),
                mint().as_str(),
            ),
        ],
    )
    .await;
    assert_eq!(second.status(), StatusCode::CREATED);
    let second = json_body(second).await;
    assert_listed_head(&case, &second, "2").await;
    // Pinning v1 selects its content; the card still describes the entry's head.
    let pinned = json_body(
        case.get(&format!(
            "{plugin}?revision_id={}",
            first["revision_id"].as_str().expect("v1")
        ))
        .await,
    )
    .await;
    assert_eq!(pinned["head"]["revision"], "1");
    assert_eq!(pinned["card"]["head_revision"], "2");
    assert_eq!(pinned["card"]["head_revision_id"], second["revision_id"]);
    let removed = send(
        &case.router,
        Method::DELETE,
        &plugin,
        Some(&case.base.token),
        "",
    )
    .await;
    assert_eq!(removed.status(), StatusCode::NO_CONTENT);
    let listed = json_body(case.get(&case.collection()).await).await;
    assert_eq!(listed["items"].as_array().expect("cards").len(), 0);
    case.base.cleanup().await;
}

/// The only listed card's ordinal and identity agree with the publication receipt.
async fn assert_listed_head(case: &Case, receipt: &Value, ordinal: &str) {
    let listed = json_body(case.get(&case.collection()).await).await;
    let cards = listed["items"].as_array().expect("cards");
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0]["head_revision"], ordinal);
    assert_eq!(cards[0]["head_revision_id"], receipt["revision_id"]);
}
