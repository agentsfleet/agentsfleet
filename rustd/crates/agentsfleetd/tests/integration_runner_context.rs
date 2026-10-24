//! Private run notes through the real authenticated router and durable storage.
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "fixture and response shape failures must fail the proof loudly"
)]
use crate::{schedules::Leased, verbs::FakeQStash};
use agentsfleetd::supervisor::Supervisor;
use reqwest::Method;
use serde_json::{Value, json};

fn path(held: &Leased) -> String {
    format!(
        "/v1/runners/me/leases/{0}/context-checkpoints/{0}",
        held.lease_id
    )
}
fn document(held: &Leased, generation: u64, goal: &str) -> Value {
    json!({"format_version":1,
        "identity":{"scope":format!("run:{}:{}:{}",held.run.fleet,held.lease_id,held.lease_id),
            "model":"selected-model-digest","pinned":"trusted-state-digest"},
        "generation":generation,"covered_through":42,"source_digest":"a".repeat(64),
        "summary":{"goal":goal,"constraints":[],"decisions":[],"completed":[],"remaining":[],"references":[]}})
}
fn put(held: &Leased, expected: u64, checkpoint: &Value) -> Value {
    json!({"fencing_token":held.fence,"expected_generation":expected,"checkpoint":checkpoint})
}
async fn stored(held: &Leased) -> Value {
    sqlx::query_scalar("SELECT document FROM memory.context_checkpoints WHERE lease_id=$1::uuid")
        .bind(&held.lease_id)
        .fetch_one(&mut *held.connection().await)
        .await
        .expect("durable run notes")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn checkpoint_retry_is_idempotent_and_superseded_assignment_cannot_read_or_replace_notes() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let held = Leased::boot(&mut supervisor, &qstash).await;
    let path = path(&held);
    let read = format!("{path}?fencing_token={}", held.fence);
    let original = document(&held, 1, "original goal");
    let body = put(&held, 0, &original);
    assert_eq!(held.call(Method::GET, &read, None).await, (200, json!({})));
    for _ in 0..2 {
        assert_eq!(held.call(Method::PUT, &path, Some(&body)).await.0, 204);
    }
    assert_eq!(stored(&held).await, original);
    assert_eq!(
        held.call(Method::GET, &read, None).await,
        (200, json!({"checkpoint":original}))
    );
    let mut foreign = document(&held, 2, "foreign scope");
    foreign["identity"]["scope"] = json!("draft:another-private-draft");
    assert_eq!(
        held.call(Method::PUT, &path, Some(&put(&held, 1, &foreign)))
            .await
            .0,
        409
    );
    let mut invalid = document(&held, 2, "invalid format");
    invalid["format_version"] = json!(99);
    assert_eq!(
        held.call(Method::PUT, &path, Some(&put(&held, 1, &invalid)))
            .await
            .0,
        400
    );
    held.supersede().await;
    assert_eq!(held.call(Method::GET, &read, None).await.0, 409);
    let changed = put(&held, 1, &document(&held, 2, "stale overwrite"));
    let (status, refusal) = held.call(Method::PUT, &path, Some(&changed)).await;
    assert_eq!(status, 409);
    assert_eq!(refusal["error_code"], "UZ-RUN-005");
    assert_eq!(stored(&held).await, original);
    held.on_fleet("DELETE FROM fleet.fleets WHERE id=$1::uuid")
        .await;
    let remaining: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM memory.context_checkpoints WHERE lease_id=$1::uuid",
    )
    .bind(&held.lease_id)
    .fetch_one(&mut *held.connection().await)
    .await
    .expect("fleet purge removes private run notes");
    assert_eq!(remaining, 0);
    held.finish(supervisor).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn one_of_one_hundred_conflicting_checkpoint_writers_publishes_exactly_one_generation() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let held = Leased::boot(&mut supervisor, &qstash).await;
    let path = path(&held);
    let barrier = tokio::sync::Barrier::new(100);
    let contenders = (0..100).map(|index| {
        let held = &held;
        let path = &path;
        let barrier = &barrier;
        async move {
            let body = put(held, 0, &document(held, 1, &format!("winner-{index}")));
            barrier.wait().await;
            held.call(Method::PUT, path, Some(&body)).await
        }
    });
    let answers = futures_util::future::join_all(contenders).await;
    assert_eq!(
        answers.iter().filter(|(status, _)| *status == 204).count(),
        1
    );
    assert_eq!(
        answers.iter().filter(|(status, _)| *status == 409).count(),
        99
    );
    let saved = stored(&held).await;
    assert_eq!(saved["generation"], 1);
    assert!(
        saved["summary"]["goal"]
            .as_str()
            .expect("winner goal")
            .starts_with("winner-")
    );
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM memory.context_checkpoints WHERE lease_id=$1::uuid",
    )
    .bind(&held.lease_id)
    .fetch_one(&mut *held.connection().await)
    .await
    .expect("one scope row");
    assert_eq!(count, 1);
    held.finish(supervisor).await;
}
