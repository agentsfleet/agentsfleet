//! The one text ceiling is reachable through the served routes: 1,048,576
//! three-byte characters make a body past axum's 2 MiB default, and the setup
//! and steer routes read it whole. One character more is refused with the
//! sentence the setup chat shows.

use super::{fixture::Fixture, json_body, mint, send};
use af_core::id::Uuid7;
use af_validate::TEXT_MAX_CHARS;
use http::{Method, StatusCode, header};
use serde_json::{Value, json};

/// Three UTF-8 bytes, one character: the ceiling of these is a 3 MiB body.
const THREE_BYTES: char = '界';

/// What the dashboard shows a person whose text is past the ceiling.
const SHOWN: &str = "Input exceeds the maximum length of 1,048,576 characters.";

/// The code a text past the ceiling is refused under.
const INPUT_TOO_LONG: &str = "UZ-REQ-004";

/// A refusal's code and the sentence the dashboard shows for it.
async fn refusal(response: axum::response::Response) -> (StatusCode, Value, Value) {
    let status = response.status();
    let problem = json_body(response).await;
    (
        status,
        problem["error_code"].clone(),
        problem["user_message"].clone(),
    )
}

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_setup_message_at_the_ceiling_in_three_byte_characters_is_accepted() {
    let f = Fixture::create().await;
    let router = f.router();
    let full: String = std::iter::repeat_n(THREE_BYTES, TEXT_MAX_CHARS).collect();
    let past = format!("{full}{THREE_BYTES}");
    assert!(full.len() > 3 * 1024 * 1024, "{} bytes", full.len());

    let mut body = f.request(&mint());
    body["message"] = json!(full);
    let created = send(
        &router,
        Method::POST,
        &f.collection(),
        Some(&f.token),
        &body.to_string(),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let item = created.headers()[header::LOCATION]
        .to_str()
        .expect("item location")
        .to_owned();
    let draft = Uuid7::parse(item.rsplit('/').next().expect("a draft id"))
        .expect("the location names a draft");
    let stored: String = sqlx::query_scalar(
        "SELECT request_json->>'message' FROM thread.turns WHERE thread_id = $1 AND sequence = 1",
    )
    .bind(&draft)
    .fetch_one(&mut *f.db.acquire().await.expect("observer"))
    .await
    .expect("the first message is stored");
    assert_eq!(stored.chars().count(), TEXT_MAX_CHARS);

    let longer = json!({"message": past, "operation_id": mint()}).to_string();
    let refused = send(
        &router,
        Method::POST,
        &format!("{item}/messages"),
        Some(&f.token),
        &longer,
    )
    .await;
    assert_eq!(
        refusal(refused).await,
        (StatusCode::BAD_REQUEST, json!(INPUT_TOO_LONG), json!(SHOWN))
    );

    // The steer route reads the same body whole: past the body layer, a
    // fleet this workspace does not hold answers 404, never 413.
    let steer = format!("/v1/workspaces/{}/fleets/{}/messages", f.workspace, mint());
    let at_ceiling = json!({"message": full}).to_string();
    let read = send(&router, Method::POST, &steer, Some(&f.token), &at_ceiling).await;
    assert_eq!(read.status(), StatusCode::NOT_FOUND);
    let over = json!({"message": past}).to_string();
    let refused = send(&router, Method::POST, &steer, Some(&f.token), &over).await;
    assert_eq!(
        refusal(refused).await,
        (StatusCode::BAD_REQUEST, json!(INPUT_TOO_LONG), json!(SHOWN))
    );
    f.cleanup().await;
}
