//! Independent destinations contend without losing jobs or serializing delivery.

use std::collections::BTreeSet;
use std::sync::Arc;

use super::*;
use tokio::sync::Barrier;
use tokio::task::JoinSet;

const DISPATCHERS: usize = 100;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn simultaneous_dispatchers_deliver_every_answer_once() {
    let token = CancellationToken::new();
    let (lanes, poster) = lanes_against(&token);
    let start = Arc::new(Barrier::new(DISPATCHERS));
    let mut dispatchers = JoinSet::new();
    for n in 0..DISPATCHERS {
        let lanes = lanes.clone();
        let start = Arc::clone(&start);
        dispatchers.spawn(async move {
            start.wait().await;
            lanes
                .dispatch(job(n, u32::try_from(n).expect("small count")))
                .await;
        });
    }
    while let Some(result) = dispatchers.join_next().await {
        result.expect("each dispatcher finishes");
    }
    await_until("every concurrent answer to be delivered", || {
        poster.delivered().len() == DISPATCHERS
    })
    .await;
    let expected: BTreeSet<Uuid7> = (0..DISPATCHERS)
        .map(|n| event(n, u32::try_from(n).expect("small count")))
        .collect();
    let delivered = poster.delivered();
    assert_eq!(delivered.len(), DISPATCHERS);
    assert_eq!(delivered.into_iter().collect::<BTreeSet<_>>(), expected);
    assert!(poster.high_water() > 1 && poster.high_water() <= IN_FLIGHT_DELIVERIES);
    await_until("every concurrent lane to retire", || lanes.active() == 0).await;
    token.cancel();
    lanes.drain().await;
}

/// The room the worker claims into shrinks with every job the lanes hold and
/// comes back as each ends, so a claim never takes a row no slot is free for.
#[tokio::test(flavor = "multi_thread")]
async fn room_shrinks_with_each_held_job_and_returns_as_each_ends() {
    let token = CancellationToken::new();
    let (lanes, poster) = lanes_against(&token);
    let gates: Vec<_> = (1..=IN_FLIGHT_DELIVERIES)
        .map(|w| poster.shut(&workspace(w)))
        .collect();
    assert_eq!(
        lanes.room(),
        IN_FLIGHT_DELIVERIES,
        "idle lanes have every slot"
    );

    for w in 1..=IN_FLIGHT_DELIVERIES {
        lanes.dispatch(job(w, 1)).await;
    }
    assert_eq!(lanes.room(), 0, "every slot is held");
    let first = gates.first().expect("one gate per slot");
    first.notify_one();
    tokio::time::timeout(PATIENCE, lanes.finished())
        .await
        .expect("the opened delivery ends");
    assert_eq!(lanes.room(), 1, "the ended job's slot is free");

    for gate in &gates {
        gate.notify_one();
    }
    await_until("every slot to come back", || {
        lanes.room() == IN_FLIGHT_DELIVERIES
    })
    .await;
    token.cancel();
    lanes.drain().await;
}
