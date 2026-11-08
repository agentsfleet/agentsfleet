//! Owner-scoped selection through the same router used by browser and terminal.

mod choices;
mod refusals;

use super::{fixture::Fixture, json_body, send};
use http::{Method, StatusCode};
use serde_json::Value;

async fn list(f: &Fixture, query: &str) -> Value {
    let path = format!("/v1/workspaces/{}/models{query}", f.workspace);
    let response = send(&f.router(), Method::GET, &path, Some(&f.token), "").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[http::header::CACHE_CONTROL],
        "private, no-store"
    );
    json_body(response).await
}
