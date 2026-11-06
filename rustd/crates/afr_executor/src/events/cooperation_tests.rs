//! Ready output notifications must spend budget before awaiting another one.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test preconditions fail loudly"
)]

use std::future::{Future as _, poll_fn};
use std::pin::pin;
use std::task::Poll;

use bytes::Bytes;

use super::Events;
use crate::{Ending, ProcessEvent, Stream};

/// Fresh spawned tasks have the same budget; measure its remainder without
/// copying Tokio's private budget constant or depending on producer scheduling.
async fn polled(events: Events) -> (Events, Poll<()>, usize) {
    let (status, remaining) = {
        let mut waiting = pin!(events.finished());
        poll_fn(|cx| {
            let status = waiting.as_mut().poll(cx);
            for remaining in 0..1_024 {
                match tokio::task::coop::poll_proceed(cx) {
                    Poll::Ready(permit) => permit.made_progress(),
                    Poll::Pending => return Poll::Ready((status, remaining)),
                }
            }
            panic!("the probe runs in a cooperatively constrained Tokio task")
        })
        .await
    };
    (events, status, remaining)
}

// Value: protects=output cannot monopolize an ending wait or consume its saved bytes;
// fails_when=ready notifications consume no cooperative budget;
// seam=Tokio's public budget probe; no_claim=continuous-producer timing or kernel cleanup.
#[tokio::test]
async fn ending_waits_cooperate_on_output_and_leave_bytes_and_endings_readable() {
    let (_idle_feed, idle) = Events::channel();
    let (live_feed, live) = Events::channel();
    live_feed.output(Stream::Stdout, Bytes::from_static(b"kept"));
    let (ended_feed, ended) = Events::channel();
    ended_feed.end(Ending::Exited(3), true);
    let (idle, live, ended) = tokio::join!(
        tokio::spawn(polled(idle)),
        tokio::spawn(polled(live)),
        tokio::spawn(polled(ended))
    );
    let (mut idle, idle_status, idle_budget) = idle.expect("idle probe");
    let (mut live, live_status, live_budget) = live.expect("output probe");
    let (mut ended, ended_status, ended_budget) = ended.expect("finished probe");
    assert_eq!(
        (idle_status, live_status, ended_status),
        (Poll::Pending, Poll::Pending, Poll::Ready(()))
    );
    assert!(idle_budget > 1);
    assert_eq!((live_budget + 1, ended_budget), (idle_budget, idle_budget));
    let (empty, finished) = idle.take_ready();
    assert!(!finished);
    assert_eq!(empty.count(), 0);
    let (output, finished) = live.take_ready();
    assert!(!finished);
    assert_eq!(
        output.collect::<Vec<_>>(),
        vec![ProcessEvent::Output {
            stream: Stream::Stdout,
            data: Bytes::from_static(b"kept")
        }]
    );
    let (ending, finished) = ended.take_ready();
    assert!(finished);
    assert_eq!(
        ending.collect::<Vec<_>>(),
        vec![ProcessEvent::Ended {
            ending: Ending::Exited(3),
            output_abandoned: true
        }]
    );
}
