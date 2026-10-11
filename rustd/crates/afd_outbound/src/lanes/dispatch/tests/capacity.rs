//! A full lane waits without borrowing the map and drops cancelled publications.

use std::future::ready;
use std::task::{Context, Poll, Waker};

use afd_core::env::MapEnv;
use afd_db::{
    Db,
    config::{DbRole, PoolConfig},
};
use afd_dragonfly::config::{DragonflyConfig, DragonflyRole};
use afd_dragonfly::{Dragonfly, OutboundDelivery, OutboundQueue};
use dashmap::try_result::TryResult;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::job;
use crate::lanes::{Destination, Job, LANE_DEPTH, Lanes};
use crate::poster::{Deliver, Posters, Verdict};

#[derive(Debug)]
struct UnusedPoster;

impl Deliver for UnusedPoster {
    fn deliver(&self, _job: &OutboundDelivery) -> impl Future<Output = Verdict> + Send {
        ready(Verdict::Delivered)
    }
}

struct FullLane {
    lanes: Lanes<UnusedPoster>,
    sender: mpsc::Sender<Job>,
    receiver: mpsc::Receiver<Job>,
    destination: Destination,
}

impl FullLane {
    fn replace(&mut self) -> mpsc::Receiver<Job> {
        let (replacement, received) = mpsc::channel(LANE_DEPTH);
        // Close and replace under the same guard as retirement, while reserve waits.
        let mut registered = self
            .lanes
            .inner
            .lanes
            .get_mut(&self.destination)
            .expect("the full lane is registered");
        self.receiver.close();
        *registered = replacement;
        received
    }
}

fn full_lane() -> FullLane {
    let environment =
        MapEnv::from_pairs([(DbRole::Api.url_knob(), "postgres://nowhere/agentsfleet")]);
    let pool = PoolConfig::resolve(&environment, DbRole::Api).expect("lazy pool config resolves");
    let queue = Dragonfly::unreachable(&DragonflyConfig::from_url(
        DragonflyRole::Default,
        "redis://127.0.0.1:1/".to_owned(),
    ))
    .expect("an unused queue handle resolves");
    let lanes = Lanes::new(
        Posters {
            slack: UnusedPoster,
        },
        OutboundQueue::new(queue),
        Db::unreachable(&pool),
        CancellationToken::new(),
    );
    let (sender, receiver) = mpsc::channel(LANE_DEPTH);
    for n in 0..LANE_DEPTH {
        sender
            .try_send(job(u32::try_from(n).expect("small lane depth")))
            .expect("the lane has exactly its declared capacity");
    }
    let destination = Destination::of(&job(0));
    // The test owns the receiver so no delivery can free capacity before polling.
    lanes
        .inner
        .lanes
        .insert(destination.clone(), sender.clone());
    FullLane {
        lanes,
        sender,
        receiver,
        destination,
    }
}

fn assert_map_writable(lane: &FullLane) {
    assert!(
        matches!(
            lane.lanes.inner.lanes.try_get_mut(&lane.destination),
            TryResult::Present(_)
        ),
        "waiting for capacity must release the lane-map guard"
    );
}

#[tokio::test]
async fn a_closed_full_lane_returns_the_waiting_job_to_its_replacement() {
    let mut lane = full_lane();
    let next = u32::try_from(LANE_DEPTH).expect("small lane depth");
    let lanes = lane.lanes.clone();
    let mut dispatch = Box::pin(lanes.dispatch(job(next)));
    let mut context = Context::from_waker(Waker::noop());
    assert!(dispatch.as_mut().poll(&mut context).is_pending());
    assert_map_writable(&lane);

    let mut received = lane.replace();
    assert!(lane.sender.is_closed());
    assert!(matches!(
        dispatch.as_mut().poll(&mut context),
        Poll::Ready(())
    ));
    drop(dispatch);

    let delivered = received
        .try_recv()
        .expect("the replacement receives the waiting job");
    assert_eq!(delivered.event_id, format!("answer-{next}"));
    assert_eq!(delivered.answer, format!("answer {next}"));
    assert!(matches!(
        received.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    ));
    for n in 0..LANE_DEPTH {
        assert_eq!(
            lane.receiver
                .try_recv()
                .expect("original queued jobs survive")
                .event_id,
            format!("answer-{n}")
        );
    }
    assert!(matches!(
        lane.receiver.try_recv(),
        Err(mpsc::error::TryRecvError::Disconnected)
    ));
    assert_eq!(received.capacity(), LANE_DEPTH);
    lane.lanes.drain().await;
}

#[tokio::test]
async fn a_full_lane_waits_without_a_map_guard_then_publishes_once() {
    let mut lane = full_lane();
    let next = u32::try_from(LANE_DEPTH).expect("small lane depth");
    let mut dispatch = Box::pin(lane.lanes.dispatch(job(next)));
    let mut context = Context::from_waker(Waker::noop());

    assert!(dispatch.as_mut().poll(&mut context).is_pending());
    assert_eq!(lane.sender.capacity(), 0);
    assert_map_writable(&lane);
    assert_eq!(
        lane.receiver
            .try_recv()
            .expect("the first queued job remains")
            .event_id,
        "answer-0"
    );
    assert!(matches!(
        dispatch.as_mut().poll(&mut context),
        Poll::Ready(())
    ));
    drop(dispatch);

    for n in 1..=LANE_DEPTH {
        assert_eq!(
            lane.receiver
                .try_recv()
                .expect("each queued job arrives once")
                .event_id,
            format!("answer-{n}")
        );
    }
    assert!(matches!(
        lane.receiver.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    ));
    assert_eq!(lane.sender.capacity(), LANE_DEPTH);
    lane.lanes.drain().await;
}

#[tokio::test]
async fn cancelling_a_full_lane_dispatch_leaves_no_job_or_reserved_capacity() {
    let mut lane = full_lane();
    let cancelled = u32::try_from(LANE_DEPTH).expect("small lane depth");
    let mut dispatch = Box::pin(lane.lanes.dispatch(job(cancelled)));
    let mut context = Context::from_waker(Waker::noop());

    assert!(dispatch.as_mut().poll(&mut context).is_pending());
    assert_map_writable(&lane);
    drop(dispatch);
    assert_map_writable(&lane);

    for n in 0..LANE_DEPTH {
        assert_eq!(
            lane.receiver
                .try_recv()
                .expect("cancellation preserves queued jobs")
                .event_id,
            format!("answer-{n}")
        );
    }
    assert!(matches!(
        lane.receiver.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    ));
    assert_eq!(lane.sender.capacity(), LANE_DEPTH);
    lane.sender
        .try_send(job(cancelled))
        .expect("cancellation leaves all capacity reusable");
    assert_eq!(
        lane.receiver
            .try_recv()
            .expect("a later send can use the lane")
            .event_id,
        format!("answer-{cancelled}")
    );
    lane.lanes.drain().await;
}
