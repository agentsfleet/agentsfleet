//! Collection cursors cannot cross audience or actor boundaries.

use super::{Case, StatusCode, json_body, mint};

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn owned_walk_is_bounded_and_rejects_a_gallery_cursor() {
    let case = Case::create().await;
    for _ in 0..2 {
        assert_eq!(
            case.publish(mint().as_str()).await.status(),
            StatusCode::CREATED
        );
    }
    let collection = case.collection();
    let first = case.get(&format!("{collection}?limit=1")).await;
    assert_eq!(first.status(), StatusCode::OK);
    let first = json_body(first).await;
    assert_eq!(first["items"].as_array().expect("first page").len(), 1);
    assert!(first["total"].is_null());
    let cursor = first["next_cursor"].as_str().expect("continuation");
    let second = case
        .get(&format!("{collection}?limit=1&starting_after={cursor}"))
        .await;
    let second = json_body(second).await;
    assert_ne!(first["items"][0]["id"], second["items"][0]["id"]);
    assert!(second["next_cursor"].is_null());
    assert_eq!(
        case.get(&format!(
            "{collection}/owned?limit=1&starting_after={cursor}"
        ))
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    for bound in ["0", "101", "-1", "1.5"] {
        assert_eq!(
            case.get(&format!("{collection}/owned?limit={bound}"))
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    let owned = json_body(case.get(&format!("{collection}/owned")).await).await;
    assert_eq!(owned["items"].as_array().expect("owned entries").len(), 2);
    case.base.cleanup().await;
}
