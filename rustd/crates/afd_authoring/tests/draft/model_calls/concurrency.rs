//! Barrier-synchronized calls share a bounded real pool and the same funded owner.

use super::fixture::{self, Provider};
use crate::draft::{attempt::Attempt, fixture::Fixture};
use afd_authoring::{ModelCallInput, ModelCalls, types::Source};
use std::{sync::Arc, time::Duration};
use tokio::{sync::Barrier, task::JoinSet, time::Instant};

const CONCURRENT: usize = 100;
const WAIT: Duration = Duration::from_secs(30);

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn hundred_pending_model_calls_release_the_pool_and_replay_without_resending() {
    let f = Fixture::create().await;
    let provider = Arc::new(Provider {
        blocked: true,
        ..Provider::known()
    });
    let base = Attempt::start(&f, f.access(), Source::Blank).await;
    let calls = fixture::calls(&f, &base, Arc::clone(&provider)).await;
    let attempts = admitted(&f, base).await;
    let barrier = Arc::new(Barrier::new(CONCURRENT + 1));
    let mut tasks = pending(&f, &calls, &attempts, &barrier);
    barrier.wait().await;
    let permits = u32::try_from(CONCURRENT).expect("cohort fits semaphore permits");
    tokio::time::timeout(WAIT, provider.entered.acquire_many(permits))
        .await
        .expect("all model sends enter together without holding the pool")
        .expect("provider boundary stays open")
        .forget();
    assert_eq!(provider.count(), CONCURRENT);
    // Every physical request is still waiting. A fresh acquisition must work.
    assert_eq!(
        tokio::time::timeout(WAIT, owned_attempts(&f))
            .await
            .expect("no connection retained across the provider wait"),
        i64::try_from(CONCURRENT).expect("cohort fits SQL count")
    );
    provider.release.add_permits(CONCURRENT);
    let durations = tokio::time::timeout(WAIT, finished(&mut tasks))
        .await
        .expect("every known final settles");
    verify_receipts(&f).await;
    replay(&f, &calls, &attempts).await;
    assert_eq!(provider.count(), CONCURRENT, "durable reads never resend");
    report(durations);
    f.cleanup().await;
}

async fn admitted(f: &Fixture, base: Attempt) -> Vec<(Attempt, ModelCallInput)> {
    let model_attempt = base.id.clone();
    let mut attempts = Vec::with_capacity(CONCURRENT);
    attempts.push((fixture::request(&base), base));
    for _ in 1..CONCURRENT {
        let attempt = Attempt::admit(f, f.access(), Source::Blank).await;
        attempts.push((fixture::request(&attempt), attempt));
    }
    sqlx::query("UPDATE authoring.attempts SET model_token_hash=(SELECT model_token_hash FROM authoring.attempts WHERE id=$2::uuid),reserved_nanos=30000000 WHERE workspace_id=$1::uuid")
        .bind(f.workspace.as_str()).bind(model_attempt.as_str())
        .execute(&mut *f.connection().await).await.expect("same valid capability for owned attempts");
    attempts
        .into_iter()
        .map(|(request, attempt)| (attempt, request))
        .collect()
}

fn pending(
    f: &Fixture,
    calls: &ModelCalls,
    attempts: &[(Attempt, ModelCallInput)],
    barrier: &Arc<Barrier>,
) -> JoinSet<Duration> {
    let mut tasks = JoinSet::new();
    for (attempt, request) in attempts {
        let (runner, lease, request, calls, barrier) = (
            f.runner.clone(),
            attempt.lease.clone(),
            request.clone(),
            calls.clone(),
            Arc::clone(barrier),
        );
        tasks.spawn(async move {
            barrier.wait().await;
            let began = Instant::now();
            let result = calls
                .call(&runner, &lease, request)
                .await
                .expect("known stream final");
            assert!(!result.replayed);
            began.elapsed()
        });
    }
    tasks
}

async fn owned_attempts(f: &Fixture) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM authoring.attempts WHERE workspace_id=$1::uuid")
        .bind(f.workspace.as_str())
        .fetch_one(&mut *f.connection().await)
        .await
        .expect("owned rows via a fresh connection")
}

async fn finished(tasks: &mut JoinSet<Duration>) -> Vec<Duration> {
    let mut durations = Vec::with_capacity(CONCURRENT);
    while let Some(result) = tasks.join_next().await {
        durations.push(result.expect("concurrent task completes"));
    }
    assert_eq!(durations.len(), CONCURRENT);
    durations
}

async fn verify_receipts(f: &Fixture) {
    let state: (i64, i64, bool, bool) = sqlx::query_as("SELECT count(d.id),sum(a.call_count)::bigint,bool_and(NOT a.usage_unknown),bool_and(d.usage->>'knowledge'='known') FROM authoring.attempts a JOIN authoring.dispatches d ON d.attempt_id=a.id WHERE a.workspace_id=$1::uuid")
        .bind(f.workspace.as_str()).fetch_one(&mut *f.connection().await)
        .await.expect("every durable accounting receipt");
    let count = i64::try_from(CONCURRENT).expect("cohort fits SQL count");
    assert_eq!(state, (count, count, true, true));
}

async fn replay(f: &Fixture, calls: &ModelCalls, attempts: &[(Attempt, ModelCallInput)]) {
    for (attempt, request) in attempts {
        assert!(
            calls
                .call(&f.runner, &attempt.lease, request.clone())
                .await
                .expect("authorized final replay")
                .replayed
        );
    }
}

fn report(mut durations: Vec<Duration>) {
    const MILLIS_PER_SECOND: f64 = 1_000.0;
    durations.sort_unstable();
    let millis = |index: usize| {
        durations
            .get(index)
            .expect("all cohort members completed")
            .as_secs_f64()
            * MILLIS_PER_SECOND
    };
    eprintln!(
        "shared_stream concurrent={CONCURRENT} completed={} p50_ms={:.3} p95_ms={:.3} p99_ms={:.3} physical_sends={CONCURRENT}",
        durations.len(),
        millis(49),
        millis(94),
        millis(98)
    );
}
