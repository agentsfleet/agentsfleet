//! A submitter's refused appends: counted, handed to the monitor, never
//! propagated.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use core::time::Duration;
use std::time::Instant;

use afd_admission::Admissions;
use afd_crypto::entropy::Entropy;
use afd_dragonfly::{Dragonfly, DragonflyConfig, DragonflyRole};
use afd_events::Steer;

use super::append_until;
use crate::abort::{Abort, CONSECUTIVE_FAILURES};

/// Port 1 is reserved and unbound, so every command is refused at once.
const NOWHERE: &str = "redis://127.0.0.1:1";

/// A Postgres nobody listens on either.
const NO_DATABASE: &str = "postgres://nobody:nobody@127.0.0.1:1/nothing?sslmode=disable";

/// How long an acquire waits before refusing: short, so a refused append
/// costs milliseconds and the monitor sees its streak inside the window.
const ACQUIRE_TIMEOUT_MS: &str = "50";

/// A steer over datastores that refuse everything, opened without a socket.
fn refusing_steer() -> Steer {
    let role = afd_db::config::DbRole::Api;
    let env = afd_core::env::MapEnv::from_pairs([
        (role.url_knob(), NO_DATABASE),
        ("DATABASE_ACQUIRE_TIMEOUT_MS", ACQUIRE_TIMEOUT_MS),
    ]);
    let pool = afd_db::config::PoolConfig::resolve(&env, role).expect("a URL resolves");
    let queue = Dragonfly::unreachable(&DragonflyConfig::from_url(
        DragonflyRole::Default,
        NOWHERE.to_owned(),
    ))
    .expect("a lazy handle opens no socket and cannot fail");
    Steer::new(Admissions::new(
        afd_db::Db::unreachable(&pool),
        queue,
        Entropy::new(),
    ))
}

#[tokio::test]
async fn a_refused_append_is_counted_and_trips_the_monitor_before_the_deadline() {
    let steer = refusing_steer();
    let fleets = [("fleet-a".to_owned(), "workspace-a".to_owned())];
    let abort = Abort::new(0.5);
    let started = Instant::now();

    let outcomes = append_until(&steer, &fleets, started + Duration::from_secs(30), &abort)
        .await
        .expect("a refusing target is counted, never propagated");

    assert!(abort.fired(), "every append refused must trip the monitor");
    assert!(
        outcomes.failures >= CONSECUTIVE_FAILURES,
        "every refusal was counted, up to the streak that stopped the run"
    );
    assert_eq!(
        outcomes.successes, 0,
        "nothing was accepted by a target that refused"
    );
    assert!(
        started.elapsed() < Duration::from_secs(25),
        "the window ended on the abort, not on its deadline"
    );
}
