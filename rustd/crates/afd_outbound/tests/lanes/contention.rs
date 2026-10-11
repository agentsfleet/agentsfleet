//! Independent destinations contend without losing jobs or serializing delivery.

use std::collections::BTreeSet;
use std::sync::Arc;

use super::*;
use tokio::sync::Barrier;
use tokio::task::JoinSet;

const DISPATCHERS: usize = 100;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn simultaneous_dispatchers_deliver_every_answer_once() {
    let server = HangingQueue::spawn().await;
    let token = CancellationToken::new();
    let (lanes, poster) = lanes_against(&server, &token).await;
    let start = Arc::new(Barrier::new(DISPATCHERS));
    let mut dispatchers = JoinSet::new();
    for n in 0..DISPATCHERS {
        let lanes = lanes.clone();
        let start = Arc::clone(&start);
        dispatchers.spawn(async move {
            start.wait().await;
            lanes
                .dispatch(job(
                    &format!("workspace-{n}"),
                    u32::try_from(n).expect("small count"),
                ))
                .await;
        });
    }
    while let Some(result) = dispatchers.join_next().await {
        result.expect("each dispatcher finishes");
    }
    await_until("every concurrent answer to be acknowledged", || {
        server.acks().len() == DISPATCHERS
    })
    .await;
    let expected: BTreeSet<String> = (0..DISPATCHERS)
        .map(|n| format!("workspace-{n}-{n}"))
        .collect();
    let delivered = poster.delivered();
    assert_eq!(delivered.len(), DISPATCHERS);
    assert_eq!(delivered.into_iter().collect::<BTreeSet<_>>(), expected);
    let expected_acks: BTreeSet<String> = (0..DISPATCHERS)
        .map(|n| format!("1700000000{n:03}-0"))
        .collect();
    let acknowledgements = server.acks();
    assert_eq!(acknowledgements.len(), DISPATCHERS);
    assert_eq!(
        acknowledgements.into_iter().collect::<BTreeSet<_>>(),
        expected_acks
    );
    assert!(poster.high_water() > 1 && poster.high_water() <= IN_FLIGHT_DELIVERIES);
    await_until("every concurrent lane to retire", || lanes.active() == 0).await;
    token.cancel();
    lanes.drain().await;
}
