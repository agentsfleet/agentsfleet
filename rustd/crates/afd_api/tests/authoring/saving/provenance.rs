//! Display provenance crosses the authenticated boundary and remains bound to one Save.

use super::{
    super::{json_body, mint},
    fixture::Case,
};
use http::StatusCode;
use serde_json::{Value, json};

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn save_records_each_supported_origin_without_relabelling_replays() {
    for origin in ["cli", "dashboard", "chat"] {
        let case = Case::create().await;
        case.fund().await;
        let (_, mut request) = case.request().await;
        request["origin"] = json!(origin);
        let key = mint();
        let response = case.save(key.as_str(), &request).await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let saved = json_body(response).await;
        let response = case.save(key.as_str(), &request).await;
        assert_eq!(response.status(), StatusCode::CREATED);
        assert_eq!(json_body(response).await["id"], saved["id"]);
        request["origin"] = json!(if origin == "cli" { "dashboard" } else { "cli" });
        let replay = case.save(key.as_str(), &request).await;
        assert_eq!(replay.status(), StatusCode::CREATED);
        assert_eq!(json_body(replay).await["id"], saved["id"]);
        let receipts: Vec<Value> = sqlx::query_scalar(
            "SELECT request_json::jsonb FROM fleet.events WHERE fleet_id=$1::uuid \
             AND event_type='activity' AND request_json::jsonb->>'kind'='setup_saved'",
        )
        .bind(saved["id"].as_str().expect("accepted fleet ID"))
        .fetch_all(&mut *case.base.db.acquire().await.expect("receipt connection"))
        .await
        .expect("durable Save receipts");
        assert_eq!(
            receipts,
            vec![json!({"kind":"setup_saved","revision":"1","origin":origin})]
        );
        case.cleanup().await;
    }
}
