//! The delivery lanes, graded without a live queue: a fake that accepts every
//! acknowledgement, and a poster whose destinations answer when the test says.
//!
//! The half of the fairness dimension that lives on the delivery side. A
//! destination that will not answer is played by a gate the test holds shut;
//! what is asserted is that answers to OTHER destinations are delivered and
//! acknowledged while it is shut, that answers to ONE destination leave in
//! the order they were queued, and that the number of vendor calls in flight
//! never passes the ceiling.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::Duration;

use afd_dragonfly::config::{RedisConfig, RedisRole};
use afd_dragonfly::streams::EventId;
use afd_dragonfly::{OutboundDelivery, OutboundQueue, Redis};
use afd_outbound::{IN_FLIGHT_DELIVERIES, Lanes, Posters};
use tokio_util::sync::CancellationToken;

#[path = "support/gated_poster.rs"]
mod gated_poster;
#[path = "support/hanging_queue.rs"]
#[allow(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]
#[allow(
    dead_code,
    reason = "shared support: the read suite grades the reads, the lane suite the acks"
)]
mod hanging_queue;
#[path = "support/no_ledger.rs"]
mod no_ledger;

use self::gated_poster::Gated;
use self::hanging_queue::HangingQueue;

/// How long a condition is waited for before the test gives up on it.
const PATIENCE: Duration = Duration::from_secs(10);

/// The gap between two looks at a condition.
const POLL: Duration = Duration::from_millis(5);

/// The deadline the client gives the fake for any one command.
const REQUEST_DEADLINE: Duration = Duration::from_secs(2);

const PROVIDER: &str = "slack";
const FLEET_ID: &str = "0199a0b0-0000-7000-8000-0000000000f1";
const SLOW_WORKSPACE: &str = "slow";
const FAST_WORKSPACE: &str = "fast";

/// A job addressed to `workspace`, numbered `n` in both of its ids.
fn job(workspace: &str, n: u32) -> Box<OutboundDelivery> {
    Box::new(OutboundDelivery {
        id: EventId::of(&format!("1700000000{n:03}-0")),
        provider: PROVIDER.to_owned(),
        workspace_id: workspace.to_owned(),
        fleet_id: FLEET_ID.to_owned(),
        event_id: format!("{workspace}-{n}"),
        answer: format!("answer {n} for {workspace}"),
    })
}

/// Lanes over a fresh fake queue, and the poster they deliver through.
async fn lanes_against(server: &HangingQueue, token: &CancellationToken) -> (Lanes<Gated>, Gated) {
    let config = RedisConfig::from_url(RedisRole::Default, server.url())
        .with_request_timeout(REQUEST_DEADLINE);
    let redis = Redis::connect(&config)
        .await
        .expect("the fake queue answers a ping");
    let poster = Gated::default();
    let lanes = Lanes::new(
        Posters {
            slack: poster.clone(),
        },
        OutboundQueue::new(redis),
        no_ledger::no_ledger(),
        token.clone(),
    );
    (lanes, poster)
}

/// Waits until `condition` holds, or fails the test naming what did not.
async fn await_until<F>(note: &str, mut condition: F)
where
    F: FnMut() -> bool,
{
    tokio::time::timeout(PATIENCE, async {
        while !condition() {
            tokio::time::sleep(POLL).await;
        }
    })
    .await
    .unwrap_or_else(|_elapsed| panic!("timed out waiting for {note}"));
}

/// A destination that will not answer holds only its own lane: answers to
/// another workspace queued BEHIND it are delivered and acknowledged while
/// it is still waiting.
#[tokio::test(flavor = "multi_thread")]
async fn a_slow_destination_does_not_hold_unrelated_answers() {
    let server = HangingQueue::spawn().await;
    let token = CancellationToken::new();
    let (lanes, poster) = lanes_against(&server, &token).await;
    let gate = poster.shut(SLOW_WORKSPACE);

    lanes.dispatch(job(SLOW_WORKSPACE, 1)).await;
    lanes.dispatch(job(FAST_WORKSPACE, 2)).await;
    lanes.dispatch(job(FAST_WORKSPACE, 3)).await;

    await_until(
        "the fast workspace's two answers to be acknowledged",
        || server.acks().len() == 2,
    )
    .await;
    assert_eq!(
        poster.delivered(),
        vec!["fast-2".to_owned(), "fast-3".to_owned()],
        "the fast answers left while the slow one was still held"
    );
    assert!(
        lanes.active() >= 1,
        "the slow lane is still open while its delivery is held"
    );

    gate.notify_one();
    await_until("the slow answer to be acknowledged", || {
        server.acks().len() == 3
    })
    .await;
    assert_eq!(
        server.acks(),
        vec![
            "1700000000002-0".to_owned(),
            "1700000000003-0".to_owned(),
            "1700000000001-0".to_owned(),
        ],
        "every delivered answer is acknowledged by its stream id"
    );

    token.cancel();
    lanes.drain().await;
}

/// Answers to one destination leave in the order they were queued, however
/// long each takes, and are acknowledged in that order.
#[tokio::test(flavor = "multi_thread")]
async fn answers_to_one_destination_leave_in_the_order_they_were_queued() {
    let server = HangingQueue::spawn().await;
    let token = CancellationToken::new();
    let (lanes, poster) = lanes_against(&server, &token).await;

    let count = 12_u32;
    for n in 1..=count {
        lanes.dispatch(job(FAST_WORKSPACE, n)).await;
    }
    await_until("every answer to be acknowledged", || {
        server.acks().len() == count as usize
    })
    .await;

    let expected: Vec<String> = (1..=count)
        .map(|n| format!("{FAST_WORKSPACE}-{n}"))
        .collect();
    assert_eq!(
        poster.delivered(),
        expected,
        "a later answer overtook an earlier one"
    );
    assert_eq!(
        poster.high_water(),
        1,
        "one destination never has two deliveries in flight"
    );

    token.cancel();
    lanes.drain().await;
}

/// Across many destinations at once, the number of deliveries in flight
/// never passes the ceiling, and every one of them is still delivered.
#[tokio::test(flavor = "multi_thread")]
async fn in_flight_deliveries_never_exceed_the_ceiling() {
    let server = HangingQueue::spawn().await;
    let token = CancellationToken::new();
    let (lanes, poster) = lanes_against(&server, &token).await;

    let destinations = 5 * IN_FLIGHT_DELIVERIES;
    for n in 0..destinations {
        let workspace = format!("workspace-{n}");
        lanes
            .dispatch(job(&workspace, u32::try_from(n).expect("small")))
            .await;
    }
    await_until("every destination's answer to be acknowledged", || {
        server.acks().len() == destinations
    })
    .await;

    assert!(
        poster.high_water() <= IN_FLIGHT_DELIVERIES,
        "{} deliveries were in flight at once; the ceiling is {IN_FLIGHT_DELIVERIES}",
        poster.high_water()
    );
    assert!(
        poster.high_water() > 1,
        "unrelated destinations delivered one at a time: the lanes are not concurrent"
    );
    await_until("every lane to retire", || lanes.active() == 0).await;

    token.cancel();
    lanes.drain().await;
}

/// A lane retires once it holds nothing, and a later answer to the same
/// destination is delivered by a fresh one — after everything the old lane
/// delivered.
#[tokio::test(flavor = "multi_thread")]
async fn a_retired_lane_is_replaced_and_order_survives_the_hand_over() {
    let server = HangingQueue::spawn().await;
    let token = CancellationToken::new();
    let (lanes, poster) = lanes_against(&server, &token).await;

    lanes.dispatch(job(FAST_WORKSPACE, 1)).await;
    await_until("the first answer to be acknowledged", || {
        server.acks().len() == 1
    })
    .await;
    await_until("the lane to retire", || lanes.active() == 0).await;

    lanes.dispatch(job(FAST_WORKSPACE, 2)).await;
    await_until("the second answer to be acknowledged", || {
        server.acks().len() == 2
    })
    .await;
    assert_eq!(
        poster.delivered(),
        vec!["fast-1".to_owned(), "fast-2".to_owned()],
        "the fresh lane delivered after the retired one"
    );

    token.cancel();
    lanes.drain().await;
}

/// Cancellation finishes the delivery in hand and acknowledges it; what was
/// still queued behind it is neither delivered nor acknowledged, so the next
/// process finds it pending.
#[tokio::test(flavor = "multi_thread")]
async fn cancellation_finishes_the_job_in_hand_and_leaves_the_rest_unacknowledged() {
    let server = HangingQueue::spawn().await;
    let token = CancellationToken::new();
    let (lanes, poster) = lanes_against(&server, &token).await;
    let gate = poster.shut(SLOW_WORKSPACE);

    lanes.dispatch(job(SLOW_WORKSPACE, 1)).await;
    lanes.dispatch(job(SLOW_WORKSPACE, 2)).await;
    lanes.dispatch(job(SLOW_WORKSPACE, 3)).await;
    await_until("the first delivery to be in flight", || {
        poster.in_flight() == 1
    })
    .await;

    token.cancel();
    gate.notify_one();
    lanes.drain().await;

    assert_eq!(
        poster.delivered(),
        vec!["slow-1".to_owned()],
        "the delivery in hand finished; nothing queued behind it was started"
    );
    assert_eq!(
        server.acks(),
        vec!["1700000000001-0".to_owned()],
        "only the finished delivery is acknowledged; the rest stay pending"
    );
}
