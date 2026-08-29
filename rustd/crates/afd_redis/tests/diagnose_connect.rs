//! What a real `Redis::connect` costs, measured rather than inferred.
//!
//! Not part of the lane. This exists because the `ConnectTimeout` failure was
//! diagnosed three times from subtraction — TLS minus TCP, then thread
//! starvation, then Docker's port proxy — and each was wrong. Plain TCP to the
//! same port is now measured at a 10.9 ms worst case both in-process and
//! through Docker, so whatever costs seconds is inside this path, not under it.
#![cfg(feature = "test-util")]

use std::time::{Duration, Instant};

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

fn report(label: &str, mut micros: Vec<u128>) {
    micros.sort_unstable();
    let at = |p: f64| micros[((micros.len() as f64 - 1.0) * p).round() as usize];
    println!(
        "{label:<26} n={:<4} p50={:<9} p90={:<9} p99={:<9} max={:<10} (microseconds)",
        micros.len(),
        at(0.50),
        at(0.90),
        at(0.99),
        micros.last().copied().unwrap_or(0),
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

    // Phase 3: concurrently, which is what the lane actually does.
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
    let mut failures = 0;
    for task in tasks {
        let (elapsed, ok) = task.await.expect("the probe task must not panic");
        if !ok {
            failures += 1;
        }
        concurrent.push(elapsed);
    }
    report("Redis::connect concurrent", concurrent);
    println!(
        "concurrent wall time = {:?}, failures = {failures}",
        started_all.elapsed()
    );

    assert_eq!(failures, 0, "no connect should fail against a healthy Redis");
    let _ = Duration::from_secs(0);
}
