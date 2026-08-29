//! What a real `Redis::connect` costs, measured rather than inferred.
//!
//! Not part of the lane. This exists because the `ConnectTimeout` failure was
//! diagnosed three times from subtraction — TLS minus TCP, then thread
//! starvation, then Docker's port proxy — and each was wrong. Plain TCP to the
//! same port is now measured at a 10.9 ms worst case both in-process and
//! through Docker, so whatever costs seconds is inside this path, not under it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::Instant;

use afd_redis::Redis;
use afd_redis::config::{RedisConfig, RedisRole};

/// The knob the LANE exports, which is not `REDIS_TLS_CA_CERT_FILE` — that one
/// is the daemon's. Reading the wrong one hands rustls no trust anchor and the
/// connect fails `UnknownIssuer`, which looks exactly like a broken server.
const CA_KNOB: &str = "TEST_REDIS_CA_CERT";

const SAMPLES: usize = 40;

fn lane(knob: &str) -> String {
    std::env::var(knob)
        .unwrap_or_else(|_| panic!("{knob} unset — run through `make test-integration-rustd`"))
}

fn config() -> RedisConfig {
    RedisConfig::from_url(RedisRole::Default, lane("TEST_REDIS_URL"))
        .with_ca_cert_file(std::env::var(CA_KNOB).ok().map(Into::into))
}

/// The value at a percentile, indexed with integers.
///
/// Computed as `(len - 1) * pct / 100` rather than through `f64`, which for a
/// Vec index buys three cast lints and a truncation question for no accuracy a
/// latency report can use.
fn at(sorted: &[u128], pct: usize) -> u128 {
    sorted
        .get(sorted.len().saturating_sub(1) * pct / 100)
        .copied()
        .unwrap_or_default()
}

fn report(label: &str, mut micros: Vec<u128>) {
    micros.sort_unstable();
    println!(
        "{label:<26} n={:<4} p50={:<9} p90={:<9} p99={:<9} max={:<10} (microseconds)",
        micros.len(),
        at(&micros, 50),
        at(&micros, 90),
        at(&micros, 99),
        micros.last().copied().unwrap_or_default(),
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "diagnostic: needs live Redis: make test-integration-rustd"]
async fn diagnose_where_connect_spends_its_time() {
    let config = config();

    // Phase 1: the synchronous half. `build_client` reads the CA off disk and
    // builds the TLS client inline on this worker -- no `spawn_blocking`.
    let mut build = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let started = Instant::now();
        let _client = afd_redis::test_util::build_client_for_diagnosis(&config);
        build.push(started.elapsed().as_micros());
    }
    report("build_client (sync half)", build);

    // Phase 2: the whole thing, sequentially, on an otherwise quiet runtime.
    let mut whole = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let started = Instant::now();
        let outcome = Redis::connect(&config).await;
        whole.push(started.elapsed().as_micros());
        assert!(outcome.is_ok(), "the lane's Redis must answer: {outcome:?}");
    }
    report("Redis::connect sequential", whole);

    // Phase 3: concurrently and UNGATED — the diagnosis, not an assertion.
    //
    // This phase measured the root cause on its first run: 40 raw connects
    // finished in 7.8 s, which is 195 ms apiece — the SAME rate as the
    // sequential phase. Concurrent connects do not parallelize: each costs
    // ~230 ms of serialized work, so simultaneous connects form a queue, and a
    // connect's 5 s budget starts before it is admitted. Everything deeper
    // than ~21 in the queue times out. 28 of 40 did. That is the lane's
    // ConnectTimeout, reproduced on demand.
    //
    // So raw failures here are the finding, not a fault: this phase REPORTS.
    let started_all = Instant::now();
    let mut tasks = Vec::new();
    for _ in 0..SAMPLES {
        let config = config.clone();
        tasks.push(tokio::spawn(async move {
            let started = Instant::now();
            let outcome = Redis::connect(&config).await;
            (started.elapsed().as_micros(), outcome.is_ok())
        }));
    }
    let mut concurrent = Vec::with_capacity(SAMPLES);
    let mut raw_failures = 0;
    for task in tasks {
        let (elapsed, ok) = task.await.expect("the probe task must not panic");
        if !ok {
            raw_failures += 1;
        }
        concurrent.push(elapsed);
    }
    report("Redis::connect concurrent", concurrent);
    println!(
        "concurrent wall time = {:?}, raw (ungated) failures = {raw_failures} of {SAMPLES}",
        started_all.elapsed()
    );

    // Phase 4: concurrently and GATED — the invariant the lane relies on.
    //
    // `connect_live` holds a one-permit semaphore across the handshake, so a
    // connect's budget starts AFTER admission and queueing time is spent
    // waiting on the permit, not on the deadline. Every lane harness routes
    // through it; this asserts what they assume: under the same concurrency
    // that fails the raw path, the gated path loses nobody.
    let started_all = Instant::now();
    let mut tasks = Vec::new();
    for _ in 0..SAMPLES {
        let config = config.clone();
        tasks.push(tokio::spawn(async move {
            let started = Instant::now();
            let outcome = afd_redis::test_util::connect_live(&config).await;
            (started.elapsed().as_micros(), outcome.is_ok())
        }));
    }
    let mut gated = Vec::with_capacity(SAMPLES);
    let mut gated_failures = 0;
    for task in tasks {
        let (elapsed, ok) = task.await.expect("the probe task must not panic");
        if !ok {
            gated_failures += 1;
        }
        gated.push(elapsed);
    }
    report("connect_live concurrent", gated);
    println!(
        "gated wall time = {:?}, failures = {gated_failures} of {SAMPLES}",
        started_all.elapsed()
    );

    assert_eq!(
        gated_failures, 0,
        "the admission gate exists so that queueing never spends a connect's \
         own budget; a failure through it is the lane's real defect"
    );
}
