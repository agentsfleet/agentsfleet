#![expect(
    clippy::unwrap_used,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use afd_wire::lease::LeaseResponse;
use afd_wire::runner::HeartbeatStatus;
use bytes::Bytes;
use tokio::sync::watch;

use super::{MIN_POLL_PAUSE, serve};
use crate::client::Verb;
use crate::error;
use crate::heartbeat::Assignment;
use crate::test_support::{
    Answer, Behaviour, FLEET_ID, FakeAgent, FakeEngine, Rig, daemon, json, lease,
};

const FIRST_LEASE: &str = "01890a5d-ac96-774b-bcce-b302099a8060";
const SECOND_LEASE: &str = "01890a5d-ac96-774b-bcce-b302099a8061";

/// A daemon whose n-th lease poll is answered by `polls(n)`.
fn polling(polls: impl Fn(usize) -> Answer + Send + Sync + 'static) -> Rig {
    polling_with(polls, FakeEngine::default())
}

fn polling_with(
    polls: impl Fn(usize) -> Answer + Send + Sync + 'static,
    engine: FakeEngine,
) -> Rig {
    let polled = AtomicUsize::new(0);
    let answer = daemon(move |call| {
        (call.verb == Verb::Lease).then(|| polls(polled.fetch_add(1, Ordering::SeqCst)))
    });
    Rig::new(answer, engine, FakeAgent::new(Behaviour::Answer))
}

fn granted(lease_id: &str) -> Answer {
    json(&LeaseResponse {
        lease: Some(lease(lease_id, FLEET_ID, None)),
        retry_after_ms: None,
    })
}

fn idle(retry_after_ms: Option<u32>) -> Answer {
    json(&LeaseResponse {
        lease: None,
        retry_after_ms,
    })
}

fn assigned(workers: u32) -> Assignment {
    Assignment {
        workers,
        ..Assignment::initial()
    }
}

async fn until(count: &AtomicUsize, at_least: usize) {
    while count.load(Ordering::SeqCst) < at_least {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test(start_paused = true)]
async fn test_worker_pool_runs_distinct_fleets() {
    let rig = polling(|polled| match polled {
        0 => granted(FIRST_LEASE),
        1 => granted(SECOND_LEASE),
        _ => idle(Some(500)),
    });
    let (_published, watching) = watch::channel(assigned(2));
    let pool = tokio::spawn(serve(Arc::clone(&rig.lessee), watching));

    until(&rig.runs, 2).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    rig.shutdown.cancel();
    pool.await.unwrap();

    assert_eq!(
        rig.runs.load(Ordering::SeqCst),
        2,
        "both of the fleet's events ran"
    );
    assert_eq!(rig.peak.load(Ordering::SeqCst), 1, "never at the same time");
}

#[tokio::test(start_paused = true)]
async fn a_refused_token_stops_the_pool() {
    let rig = polling(|_| Answer::Fail(error::refused(Verb::Lease, 401, None)));
    let (_published, watching) = watch::channel(assigned(1));

    serve(Arc::clone(&rig.lessee), watching).await;

    assert!(rig.lessee.halt.token_refused());
}

#[tokio::test(start_paused = true)]
async fn failed_polls_back_off_and_a_zero_hint_is_floored() {
    let polls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&polls);
    let rig = polling(move |polled| {
        counted.fetch_add(1, Ordering::SeqCst);
        match polled {
            0..=5 => Answer::Fail(error::unavailable(Verb::Lease, 503)),
            6 => Answer::Reply(Bytes::from_static(b"[")),
            7 => granted("nope"),
            _ => idle(Some(0)),
        }
    });
    let (published, watching) = watch::channel(assigned(1));
    let pool = tokio::spawn(serve(Arc::clone(&rig.lessee), watching));

    tokio::time::sleep(Duration::from_secs(2)).await;
    let early = polls.load(Ordering::SeqCst);
    until(&polls, 9).await;
    let floored = polls.load(Ordering::SeqCst);
    tokio::time::sleep(MIN_POLL_PAUSE * 4).await;
    let later = polls.load(Ordering::SeqCst);
    published.send_replace(Assignment {
        status: HeartbeatStatus::Drain,
        ..assigned(2)
    });
    drop(published);
    pool.await.unwrap();

    assert!(
        early < 6,
        "six blips took longer than two seconds to spend: {early}"
    );
    assert!(
        later - floored <= 5,
        "a zero hint polls at most every {MIN_POLL_PAUSE:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn a_worker_that_panics_is_started_again() {
    let engine = FakeEngine {
        panic_once: true,
        ..FakeEngine::default()
    };
    let rig = polling_with(
        |polled| match polled {
            0 => granted(FIRST_LEASE),
            1 => granted(SECOND_LEASE),
            _ => idle(Some(500)),
        },
        engine,
    );
    let (_published, watching) = watch::channel(assigned(1));
    let pool = tokio::spawn(serve(Arc::clone(&rig.lessee), watching));

    until(&rig.runs, 1).await;
    rig.shutdown.cancel();
    pool.await.unwrap();

    assert_eq!(
        rig.prepared.load(Ordering::SeqCst),
        2,
        "the second lease ran on a fresh worker"
    );
}

/// Once a lease leaves its sandbox held, the next poll names that fleet, so
/// the daemon offers the fleet's next event here first.
#[tokio::test(start_paused = true)]
async fn test_a_poll_after_a_park_names_the_held_fleet() {
    const HOLDS: &str = "holds";
    let polls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&polls);
    let mut rig = polling(move |polled| {
        counted.fetch_add(1, Ordering::SeqCst);
        match polled {
            0 => granted(FIRST_LEASE),
            _ => idle(Some(500)),
        }
    });
    let (_published, watching) = watch::channel(assigned(1));
    let pool = tokio::spawn(serve(Arc::clone(&rig.lessee), watching));

    until(&polls, 2).await;
    rig.shutdown.cancel();
    pool.await.unwrap();

    let held: Vec<Option<serde_json::Value>> = rig
        .calls()
        .iter()
        .filter(|call| call.verb == Verb::Lease)
        .take(2)
        .map(|call| {
            let body = call.body.as_ref().unwrap();
            let sent: serde_json::Value = serde_json::from_slice(body).unwrap();
            sent.get(HOLDS).cloned()
        })
        .collect();
    assert_eq!(
        held,
        [
            Some(serde_json::json!([])),
            Some(serde_json::json!([FLEET_ID]))
        ],
        "nothing held before the first lease, its fleet after"
    );
}
