//! Force retirement after capacity reservation, before job publication.
#![expect(
    clippy::expect_used,
    reason = "a failed test precondition must stop the test"
)]

use super::*;
use crate::lanes::LANE_DEPTH;
use afd_dragonfly::{OutboundDelivery, streams::EventId};

mod capacity;

fn job(number: u32) -> Job {
    Box::new(OutboundDelivery {
        id: EventId::of(&format!("1700000000{number:03}-0")),
        provider: "slack".to_owned(),
        destination: "thread".to_owned(),
        workspace_id: "workspace".to_owned(),
        fleet_id: "fleet".to_owned(),
        event_id: format!("answer-{number}"),
        answer: format!("answer {number}"),
    })
}

#[test]
fn a_current_lane_receives_a_reserved_job_once() {
    let lanes = DashMap::new();
    let first = job(1);
    let destination = Destination::of(&first);
    let (sender, mut receiver) = mpsc::channel(LANE_DEPTH);
    lanes.insert(destination.clone(), sender.clone());
    let permit = sender.try_reserve().expect("an empty lane has capacity");

    publish(&lanes, &destination, &sender, permit, first).expect("the lane is current");

    assert_eq!(
        receiver
            .try_recv()
            .expect("the job reached its lane")
            .event_id,
        "answer-1"
    );
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    ));
}

#[test]
fn a_reserved_job_survives_removal_of_its_lane() {
    let lanes = DashMap::new();
    let first = job(1);
    let destination = Destination::of(&first);
    let (sender, mut receiver) = mpsc::channel(LANE_DEPTH);
    lanes.insert(destination.clone(), sender.clone());
    let permit = sender.try_reserve().expect("an empty lane has capacity");
    receiver.close();
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    ));
    lanes.remove(&destination);
    drop(receiver);

    let returned = publish(&lanes, &destination, &sender, permit, first)
        .expect_err("retirement must return the job rather than discard it")
        .0;
    assert_eq!(returned.event_id, "answer-1");
    let (fresh, mut receiver) = mpsc::channel(LANE_DEPTH);
    lanes.insert(destination.clone(), fresh.clone());
    let permit = fresh.try_reserve().expect("the fresh lane has capacity");
    publish(&lanes, &destination, &fresh, permit, returned).expect("retry uses the fresh lane");
    let permit = fresh
        .try_reserve()
        .expect("the fresh lane has more capacity");
    publish(&lanes, &destination, &fresh, permit, job(2)).expect("the next job queues behind it");

    assert_eq!(
        receiver
            .try_recv()
            .expect("the first job survived")
            .event_id,
        "answer-1"
    );
    assert_eq!(
        receiver
            .try_recv()
            .expect("the second job follows it")
            .event_id,
        "answer-2"
    );
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    ));
}

#[test]
fn a_reserved_job_cannot_publish_to_a_replaced_lane() {
    let lanes = DashMap::new();
    let first = job(1);
    let destination = Destination::of(&first);
    let (sender, mut receiver) = mpsc::channel(LANE_DEPTH);
    lanes.insert(destination.clone(), sender.clone());
    let permit = sender.try_reserve().expect("an empty lane has capacity");
    receiver.close();
    drop(receiver);
    let (fresh, mut receiver) = mpsc::channel(LANE_DEPTH);
    lanes.insert(destination.clone(), fresh.clone());

    let returned = publish(&lanes, &destination, &sender, permit, first)
        .expect_err("a stale permit must not accept the job")
        .0;
    assert_eq!(returned.event_id, "answer-1");
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    ));
    let permit = fresh.try_reserve().expect("the replacement has capacity");
    publish(&lanes, &destination, &fresh, permit, returned).expect("retry reaches the replacement");

    assert_eq!(
        receiver.try_recv().expect("the job arrived once").event_id,
        "answer-1"
    );
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    ));
}
