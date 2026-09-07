//! The abort rail, proven wired into a lane rather than only into itself.
//!
//! `abort/tests.rs` proves the monitor fires past its threshold. This proves
//! a LANE'S loop feeds it and stops when it does: a lease driver over
//! datastores nobody listens on refuses every poll, and after the minimum
//! sample the run ends with the abort recorded rather than spinning until its
//! deadline.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use core::time::Duration;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Instant;

use afd_bench::abort::{Abort, MINIMUM_SAMPLE};
use afd_bench::lane::lease::drive::{Shared, poll_until};
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_fleet::lease::Leases;
use afd_redis::{Redis, RedisConfig, RedisRole};

/// Port 1 is reserved and unbound on every platform this builds for, so a
/// command fails on refusal in microseconds rather than waiting out a timeout.
const NOWHERE: &str = "redis://127.0.0.1:1";

/// Any v7-shaped runner id; the pass never reaches the row it would name.
const RUNNER: &str = "0195b4ba-8d3a-7001-8abc-000000000001";

/// A Postgres nobody listens on either; the pass fails at the peek before it.
const NO_DATABASE: &str = "postgres://nobody:nobody@127.0.0.1:1/nothing?sslmode=disable";

/// A pool config over [`NO_DATABASE`], resolved the way a lane resolves one.
fn no_pool() -> afd_db::config::PoolConfig {
    let role = afd_db::config::DbRole::Api;
    let env = afd_core::env::MapEnv::from_pairs([(role.url_knob(), NO_DATABASE)]);
    afd_db::config::PoolConfig::resolve(&env, role).expect("a URL resolves")
}

#[tokio::test]
async fn test_a_run_aborts_when_the_target_starts_failing() {
    let queue = Redis::unreachable(&RedisConfig::from_url(
        RedisRole::Default,
        NOWHERE.to_owned(),
    ))
    .expect("a lazy handle opens no socket and cannot fail");
    let leases = Leases::new(afd_db::Db::unreachable(&no_pool()), queue, Entropy::new());
    let runner = Uuid7::parse(RUNNER).expect("a v7 spelling");
    let abort = Arc::new(Abort::new(0.5));
    let shared = Shared {
        deadline: Instant::now() + Duration::from_secs(30),
        leased: AtomicU64::new(0),
        stop_after: None,
        abort: Arc::clone(&abort),
    };

    let started = Instant::now();
    let (outcomes, last_lease) = poll_until(&leases, &runner, &shared)
        .await
        .expect("a refusing target is counted, never propagated");

    assert!(abort.fired(), "every poll refused must trip the monitor");
    assert!(
        outcomes.failures >= afd_bench::abort::CONSECUTIVE_FAILURES.min(MINIMUM_SAMPLE),
        "the monitor judges only after its sample"
    );
    assert_eq!(
        outcomes.attempts(),
        0,
        "nothing was measured off a target that refused"
    );
    assert!(last_lease.is_none(), "no lease was ever issued");
    assert!(
        started.elapsed() < Duration::from_secs(25),
        "the run stopped on the abort, not on its deadline"
    );
    let recorded = abort
        .recorded()
        .expect("a fired abort is what the result file records");
    assert!((recorded.observed_error_rate - 1.0).abs() < f64::EPSILON);
}
