//! A real runner token drives both web endpoints on an assigned restricted fleet.
use af_core::error_code::{INVALID_REQUEST, RUN_STALE_FENCING_TOKEN, WEB_NETWORK_RESTRICTED};
use agentsfleetd::supervisor::Supervisor;
use reqwest::Method;
use serde_json::{Value, json};

use crate::schedules::Assigned;
use crate::verbs::FakeQStash;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn web_routes_enforce_the_live_fence_and_network_before_reserving() -> Result<(), sqlx::Error>
{
    let mut supervisor = Supervisor::new();
    let scheduler = FakeQStash::start().await;
    let held = Box::pin(Assigned::boot(&mut supervisor, &scheduler)).await;
    let fence = held.fence;
    let query = |fence| json!({"fencing_token":fence,"call_number":1,"query":"migration guide"});
    let page =
        |fence| json!({"fencing_token":fence,"call_number":2,"url":"https://example.org/guide"});
    let unknown =
        json!({"fencing_token":fence,"call_number":3,"query":"guide","api_key":"forbidden"});
    for (suffix, body, status, expected) in [
        ("searches", query(fence), 403, WEB_NETWORK_RESTRICTED),
        ("extracts", page(fence), 403, WEB_NETWORK_RESTRICTED),
        ("searches", query(fence + 1), 409, RUN_STALE_FENCING_TOKEN),
        ("extracts", page(fence + 1), 409, RUN_STALE_FENCING_TOKEN),
        ("searches", unknown, 400, INVALID_REQUEST),
    ] {
        let path = format!("/v1/runners/me/runs/{}/{suffix}", held.run_id);
        let (actual, problem) = held.call(Method::POST, &path, Some(&body)).await;
        assert_eq!(actual, status);
        assert_eq!(
            problem.get("error_code").and_then(Value::as_str),
            Some(expected.as_str())
        );
        assert!(problem.get("local_read_permitted").is_none());
    }
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM thread.run_web_calls WHERE run_id = $1")
            .bind(&held.run_id)
            .fetch_one(&mut *held.connection().await)
            .await?;
    assert_eq!(count, 0);
    held.finish(supervisor).await;
    Ok(())
}
