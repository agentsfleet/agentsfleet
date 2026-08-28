//! What the retention sweeper decides without touching a row.
//!
//! The deletes themselves are statements, proven in the integration lane
//! against real rows. What is decidable here is the PACING — a cycle that
//! drained everything waits an hour, and one that filled every batch comes back
//! in a minute — and the window's relationship to a lease's maximum life, which
//! the compiler already checked and this states where a reader looks for it.
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use std::sync::atomic::Ordering;

use afd_core::env::MapEnv;
use afd_core::timing::MAX_RUNTIME_MS;
use afd_db::Db;
use afd_db::config::{DbRole, PoolConfig};

use super::{IDLE_INTERVAL, RETENTION_WINDOW_MS, Retention, SATURATED_INTERVAL};
use crate::sweep::Sweep as _;

/// A sweeper over a pool nothing here ever asks anything of.
///
/// Every case below reads pacing state and runs no statement, so the pool is a
/// value the type needs rather than a datastore under test — which is why the
/// unreachable one is exactly right. It is still built inside a runtime,
/// because a lazy `sqlx` pool registers with the reactor the moment it exists.
fn sweeper() -> Retention {
    let environment =
        MapEnv::from_pairs([(DbRole::Api.url_knob(), "postgres://nowhere/agentsfleet")]);
    let pool = PoolConfig::resolve(&environment, DbRole::Api).expect("a lazy pool config resolves");
    Retention::new(Db::unreachable(&pool))
}

#[tokio::test]
async fn a_drained_cycle_waits_an_hour_and_a_saturated_one_comes_straight_back() {
    // The failure this prevents: a backlog that outran one cycle's ceiling
    // would otherwise idle a full hour, capping throughput while every cycle
    // reports success and the table grows underneath it.
    let retention = sweeper();
    assert_eq!(retention.interval(), IDLE_INTERVAL);

    retention.saturated.store(true, Ordering::Relaxed);
    assert_eq!(retention.interval(), SATURATED_INTERVAL);
    assert!(SATURATED_INTERVAL < IDLE_INTERVAL);

    // And it idles again once the backlog is gone, so one busy cycle does not
    // pin the sweeper at the fast rate for the life of the process.
    retention.saturated.store(false, Ordering::Relaxed);
    assert_eq!(retention.interval(), IDLE_INTERVAL);
}

/// The premise the age-keyed design rests on, restated where a reader looks
/// for it.
///
/// NOT a runtime test: both sides are constants, so the module's own
/// `const _: () = assert!(..)` already fails the BUILD if the ceiling ever
/// grows past the window. A runtime assertion over two constants is a check
/// that can only pass, and clippy is right to refuse it — this comment is what
/// the test was for.
const _: () = assert!(MAX_RUNTIME_MS < RETENTION_WINDOW_MS);
