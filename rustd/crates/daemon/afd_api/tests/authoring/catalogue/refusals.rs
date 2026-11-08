//! Malformed filters and foreign continuations never become an unfiltered first page.

use super::{Fixture, list};
use crate::harness::send;
use http::{Method, StatusCode};

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn model_choices_refuse_invalid_filters_and_bound_their_cursors() {
    let f = Fixture::create().await;
    let router = f.router();
    for query in [
        "?purpose=other",
        "?limit=0",
        "?limit=101",
        "?page=2",
        "?starting_after=foreign",
    ] {
        let path = format!("/v1/workspaces/{}/models{query}", f.workspace);
        let response = send(&router, Method::GET, &path, Some(&f.token), "").await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{query}");
    }
    let first = list(&f, "?limit=1").await;
    if let Some(cursor) = first["next_cursor"].as_str() {
        let path = format!(
            "/v1/workspaces/{}/models?limit=1&purpose=authoring&starting_after={cursor}",
            f.workspace
        );
        let response = send(&router, Method::GET, &path, Some(&f.token), "").await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
    f.cleanup().await;
}
