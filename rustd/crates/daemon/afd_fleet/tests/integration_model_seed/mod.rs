//! Concurrent catalogue setup shares logical models without sharing candidate ids.
#![expect(
    clippy::expect_used,
    reason = "test prerequisites and failures must stop the scenario"
)]

use std::{
    panic::AssertUnwindSafe,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

use futures_util::{FutureExt as _, future::join_all};
use tokio::sync::Barrier;

use crate::support::Fixtures;

const CALLERS: usize = 100;
const PROVIDER: &str = "fixture-catalogue";
const MODELS: [&str; 2] = ["catalogue-first", "catalogue-second"];
const READ_MODELS: &str = include_str!("fixtures/read_models.sql");

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn concurrent_seeds_preserve_two_models_and_their_prices() {
    let fixtures = Fixtures::create_isolated_with_queue().await;
    let outcome = AssertUnwindSafe(tokio::time::timeout(
        Duration::from_secs(30),
        seed_and_read(&fixtures),
    ))
    .catch_unwind()
    .await;
    tokio::time::timeout(Duration::from_secs(10), fixtures.cleanup())
        .await
        .expect("owned fixture cleanup remains bounded after failure");
    match outcome {
        Ok(result) => result.expect("concurrent catalogue setup is bounded"),
        Err(failure) => std::panic::resume_unwind(failure),
    }
}

async fn seed_and_read(fixtures: &Fixtures) {
    let barrier = Barrier::new(CALLERS);
    let active = AtomicUsize::new(0);
    let peak = AtomicUsize::new(0);
    join_all(MODELS.into_iter().cycle().take(CALLERS).map(|model| {
        let barrier = &barrier;
        let active = &active;
        let peak = &peak;
        async move {
            barrier.wait().await;
            let in_flight = active.fetch_add(1, Ordering::Relaxed) + 1;
            peak.fetch_max(in_flight, Ordering::Relaxed);
            fixtures.seed_model_rate(PROVIDER, model).await;
            active.fetch_sub(1, Ordering::Relaxed);
        }
    }))
    .await;
    assert!(peak.load(Ordering::Relaxed) > 1, "seeds overlapped");
    assert_eq!(active.load(Ordering::Relaxed), 0, "every seed finished");

    let expected: Vec<_> = MODELS
        .into_iter()
        .map(|model| {
            (
                model.to_owned(),
                200_000_i32,
                3_000_000_000_i64,
                300_000_000_i64,
                15_000_000_000_i64,
                serde_json::json!({
                    "api": "messages",
                    "base_url": "https://api.anthropic.com/v1",
                    "max_output_tokens": 8192
                }),
            )
        })
        .collect();
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("catalogue connection");
    let rows = sqlx::query_as::<_, (String, i32, i64, i64, i64, serde_json::Value)>(READ_MODELS)
        .bind(PROVIDER)
        .fetch_all(&mut *connection)
        .await
        .expect("model identities and their quotes remain readable");
    assert_eq!(
        rows, expected,
        "each logical model has exactly one complete quote"
    );
}
