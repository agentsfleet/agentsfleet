//! A revoked runner's retirement removes its operator-visible record and history.

use super::{Fixture, send};
use http::{Method, StatusCode};

pub(super) async fn assert_retired(router: &axum::Router, fixture: &Fixture, path: &str) {
    let response = send(router, Method::DELETE, path, Some(&fixture.token), "").await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    for method in [Method::GET, Method::DELETE] {
        let response = send(router, method, path, Some(&fixture.token), "").await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
    let mut connection = fixture.database.acquire().await.expect("an API connection");
    let events: i64 =
        sqlx::query_scalar("SELECT count(*) FROM runner.events WHERE runner_id = $1::uuid")
            .bind(&fixture.runner)
            .fetch_one(&mut *connection)
            .await
            .expect("the scoped runner events remain readable");
    assert_eq!(events, 0);
}
