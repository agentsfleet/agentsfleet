//! Receipt pages exclude runtime bodies before the page limit is applied.

use super::super::super::send;
use super::fixture::Saved;
use http::Method;
use http::StatusCode;
use serde_json::{Value, json};
use std::collections::BTreeSet;

#[tokio::test]
#[ignore = "needs isolated local Postgres and Dragonfly"]
async fn receipt_filters_decode_and_validate_before_reading() {
    let saved = Saved::create(json!({})).await;
    let encoded = saved.get("/messages?event_type=%61ctivity&limit=25").await;
    assert_eq!(
        encoded["items"]
            .as_array()
            .expect("decoded receipt page")
            .len(),
        2
    );
    for value in ["", "%00", "activity&event_type=chat", &"x".repeat(65)] {
        let response = send(
            &saved.reloaded_router(),
            Method::GET,
            &format!("{}/messages?event_type={value}", saved.item),
            Some(&saved.case.base.token),
            "",
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "invalid decoded filter {value}"
        );
    }
    saved.case.cleanup().await;
}

#[tokio::test]
#[ignore = "needs isolated local Postgres and Dragonfly"]
async fn busy_fleets_keep_exact_receipts_across_filtered_pages() {
    let saved = Saved::create(json!({})).await;
    for index in 0..26 {
        assert_eq!(
            saved.memory(index % 2 == 0, false).await.status(),
            StatusCode::OK
        );
    }
    sqlx::query("INSERT INTO fleet.events
        (fleet_id,workspace_id,event_id,actor,event_type,status,request_json,response_text,created_at,updated_at)
        SELECT $1::uuid,$2::uuid,'busy-'||n,'steer:api','chat','processed',
          jsonb_build_object('message',repeat('runtime body ',5000)),'Runtime reply',now(),now()
        FROM generate_series(1,33) n")
        .bind(saved.fleet.as_str()).bind(saved.case.base.workspace.as_str())
        .execute(&mut *saved.case.base.db.acquire().await.expect("busy fleet connection"))
        .await.expect("runtime rows newer than receipts");
    let first = saved.get("/messages?event_type=activity&limit=25").await;
    assert_eq!(first["items"].as_array().expect("first page").len(), 25);
    let mut identities = BTreeSet::new();
    receipts(&first, &mut identities);
    let cursor = first["next_cursor"].as_str().expect("three older receipts");
    let second = saved
        .get(&format!(
            "/messages?event_type=activity&limit=25&starting_after={cursor}"
        ))
        .await;
    assert_eq!(second["items"].as_array().expect("second page").len(), 3);
    receipts(&second, &mut identities);
    assert_eq!(
        identities.len(),
        28,
        "26 memory changes plus one saved/ready pair"
    );
    assert!(second["next_cursor"].is_null());
    saved.case.cleanup().await;
}

fn receipts(page: &Value, identities: &mut BTreeSet<String>) {
    for row in page["items"].as_array().expect("receipt rows") {
        assert_eq!(row["event_type"], "activity");
        assert!(row["response_text"].is_null());
        assert!(!row.to_string().contains("runtime body"));
        assert!(identities.insert(row["event_id"].as_str().expect("event identity").to_owned()));
    }
}
