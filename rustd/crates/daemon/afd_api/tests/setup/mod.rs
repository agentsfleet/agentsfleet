//! Native setup over the full HTTP router and a real Postgres database.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "unmet integration preconditions fail loudly, and a step indexes the JSON \
              and headers it just received"
)]

mod catalogue;
mod ceiling;
mod fixture;
mod input;
mod lifecycle;
mod privacy;
mod readiness;
mod saving;
mod seed;
mod stream;

use crate::harness::{json_body, send, send_with_headers};
use af_core::id::Uuid7;
use af_core::money::NANOS_PER_USD;

/// What a funded fixture wallet holds: a thousand dollars, past any reservation
/// a fixture turn or activation can make.
const FUNDED_NANOS: i64 = 1_000 * NANOS_PER_USD;

fn mint() -> Uuid7 {
    afd_db::test_util::mint_id()
}

/// The identifier a response field carries, which a statement binds as a
/// `uuid` rather than as the text the JSON spelled it in.
fn id_field(field: &serde_json::Value) -> Uuid7 {
    Uuid7::parse(field.as_str().expect("an identifier field is a string"))
        .expect("an identifier field is a UUIDv7")
}

/// Stops the draft turn at `sequence` with the turn `PATCH`, as `token`:
/// `item` is the draft's path, and its messages name the turn.
async fn stop(
    router: &axum::Router,
    item: &str,
    token: &str,
    sequence: &str,
) -> axum::response::Response {
    let messages = send(
        router,
        http::Method::GET,
        &format!("{item}/messages"),
        Some(token),
        "",
    )
    .await;
    assert_eq!(messages.status(), http::StatusCode::OK);
    let messages = json_body(messages).await;
    let turn = messages["items"]
        .as_array()
        .expect("a message page holds items")
        .iter()
        .find(|message| message["sequence"] == sequence)
        .expect("the draft holds a message at that sequence")["turn_id"]
        .as_str()
        .expect("a message names its turn")
        .to_owned();
    send(
        router,
        http::Method::PATCH,
        &format!("{item}/turns/{turn}"),
        Some(token),
        r#"{"status":"cancelled"}"#,
    )
    .await
}
