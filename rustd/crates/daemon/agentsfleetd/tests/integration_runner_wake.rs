//! The held heartbeat against the booted daemon: a beat that asks to be held
//! answers `woken` once fresh work is admitted, through the production lease
//! plane and its doorbell rather than a stub.
#![cfg(feature = "test-util")]
#![allow(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::{Duration, Instant};

use afd_core::timing::WAKE_HOLD_MAX;
use agentsfleetd::supervisor::Supervisor;
use serde_json::Value;

use crate::e2e::{EVENT_TYPE, scenario};
use crate::wire::{capable_beat, post};

const HEARTBEATS: &str = "/v1/runners/me/heartbeats";

/// How often fresh work is admitted while the beat is held. A ring that lands
/// before the hold begins wakes nobody, so admissions repeat until one lands
/// inside it.
const ADMIT_EVERY: Duration = Duration::from_millis(250);

/// A beat asking to be held is answered `woken: true` by the ring an admitted
/// event sends, well before the hold's deadline would have answered it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_a_held_heartbeat_is_woken_by_admitted_work() {
    let mut supervisor = Supervisor::new();
    let run = scenario(&mut supervisor).await;
    #[expect(
        clippy::disallowed_methods,
        reason = "a plain client calls the daemon this test booted; a proxy rule and a deadline are not what the test proves"
    )]
    let http = reqwest::Client::new();
    let mut asking = capable_beat();
    asking
        .as_object_mut()
        .expect("a beat is a JSON object")
        .insert("wake".to_owned(), Value::Bool(true));

    let began = Instant::now();
    let answered = {
        let beat = post(&http, &run, HEARTBEATS, &asking);
        tokio::pin!(beat);
        loop {
            tokio::select! {
                answered = &mut beat => break answered,
                () = tokio::time::sleep(ADMIT_EVERY) => {
                    run.enqueue_event(EVENT_TYPE).await;
                }
            }
        }
    };
    let waited = began.elapsed();
    let status = answered.status().as_u16();
    let reply: Value = answered.json().await.expect("a heartbeat reply is JSON");

    assert_eq!(
        (status, reply.get("woken"), waited < WAKE_HOLD_MAX),
        (200, Some(&Value::Bool(true)), true),
        "woken by a ring after {waited:?}, not by the {WAKE_HOLD_MAX:?} deadline"
    );
    supervisor.shutdown().await;
    run.cleanup().await;
}
