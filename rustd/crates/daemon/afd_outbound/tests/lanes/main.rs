//! The delivery lanes, graded without a live ledger: a handle whose every
//! write fails, and a poster whose destinations answer when the test says.
//!
//! The half of the fairness dimension that lives on the delivery side. A
//! destination that will not answer is played by a gate the test holds shut;
//! what is asserted is that answers to OTHER destinations are delivered while
//! it is shut, that answers to ONE destination leave in
//! the order they were queued, and that the number of vendor calls in flight
//! never passes the ceiling.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::Duration;

use af_core::id::Uuid7;
use afd_outbound::{Claimed, IN_FLIGHT_DELIVERIES, Lanes, Outgoing, Posters};
use tokio_util::sync::CancellationToken;

#[path = "../support/gated_poster.rs"]
mod gated_poster;
#[path = "../support/no_ledger.rs"]
mod no_ledger;

use self::gated_poster::Gated;

mod contention;
mod ledger_outage;
mod shutdown;

/// The thread an owed answer is addressed to, as a Slack producer records it.
const DESTINATION: &str = r#"{"channel_id":"C0123456789","thread_ts":"1700000000.000100"}"#;

/// How long a condition is waited for before the test gives up on it.
///
/// Harness patience, NOT a budget any of these tests assert. Nothing here
/// claims a latency: the claims are the ceiling (`high_water <= ceiling`), the
/// concurrency (`high_water > 1`) and that every answer is delivered. Those
/// hold or fail regardless of how long the wait is allowed to be, which is why
/// this number can be generous without softening anything.
///
/// Generous because the coverage lane runs the whole workspace at once and
/// every `multi_thread` test spawns workers per core, so a small runner ends up
/// heavily oversubscribed. Measured on this machine,
/// `in_flight_deliveries_never_exceed_the_ceiling` finishes in 0.19 s both
/// plain and under `cargo llvm-cov` — instrumentation is not the cost,
/// contention is. Continuous Integration timed out at ten seconds anyway, so
/// the wait now carries the same thirty-second ceiling `abort_wiring.rs` uses
/// and still fails loudly on a real stall rather than a slow neighbour.
const PATIENCE: Duration = Duration::from_secs(30);

/// The gap between two looks at a condition.
const POLL: Duration = Duration::from_millis(5);

const PROVIDER: &str = "slack";
const FLEET_ID: &str = "0199a0b0-0000-7000-8000-0000000000f1";
/// The workspace whose destination a test holds shut, by number.
const SLOW_WORKSPACE: usize = 1;
/// The workspace whose destination answers, by number.
const FAST_WORKSPACE: usize = 2;

/// A fixture identifier spelled in full.
fn id(text: &str) -> Uuid7 {
    Uuid7::parse(text).expect("a fixture identifier is a canonical UUIDv7")
}

/// Workspace number `w`, so a test can address as many as it needs.
fn workspace(w: usize) -> Uuid7 {
    id(&format!("0199a0b0-0000-7000-8000-{w:012x}"))
}

/// The event job `n` to workspace `w` answers. Both numbers are in it, so a
/// delivery log names the workspace as well as the job.
fn event(w: usize, n: u32) -> Uuid7 {
    id(&format!("0199a0b0-{w:04x}-7000-8000-{n:012x}"))
}

/// A job addressed to workspace `w`, numbered `n` in both of its ids.
fn job(w: usize, n: u32) -> Box<Claimed> {
    Box::new(Claimed {
        id: event(w, n),
        token: af_core::id::Uuid7::parse("0195b4ba-8d3a-7000-8abc-0000000000ff")
            .expect("a fixture token"),
        outgoing: Outgoing {
            provider: PROVIDER.to_owned(),
            destination: DESTINATION.to_owned(),
            workspace_id: workspace(w),
            fleet_id: id(FLEET_ID),
            event_id: event(w, n),
            answer: format!("answer {n} for workspace {w}"),
        },
    })
}

/// Lanes over no ledger, and the poster they deliver through.
fn lanes_against(token: &CancellationToken) -> (Lanes<Gated>, Gated) {
    let poster = Gated::default();
    let lanes = Lanes::new(
        Posters {
            slack: poster.clone(),
        },
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
/// another workspace queued BEHIND it are delivered while it is still
/// waiting.
#[tokio::test(flavor = "multi_thread")]
async fn a_slow_destination_does_not_hold_unrelated_answers() {
    let token = CancellationToken::new();
    let (lanes, poster) = lanes_against(&token);
    let gate = poster.shut(&workspace(SLOW_WORKSPACE));

    lanes.dispatch(job(SLOW_WORKSPACE, 1)).await;
    lanes.dispatch(job(FAST_WORKSPACE, 2)).await;
    lanes.dispatch(job(FAST_WORKSPACE, 3)).await;

    await_until("the fast workspace's two answers to be delivered", || {
        poster.delivered().len() == 2
    })
    .await;
    assert_eq!(
        poster.delivered(),
        vec![event(FAST_WORKSPACE, 2), event(FAST_WORKSPACE, 3)],
        "the fast answers left while the slow one was still held"
    );
    assert!(
        lanes.active() >= 1,
        "the slow lane is still open while its delivery is held"
    );

    gate.notify_one();
    await_until("the slow answer to be delivered", || {
        poster.delivered().len() == 3
    })
    .await;
    assert_eq!(
        poster.delivered(),
        vec![
            event(FAST_WORKSPACE, 2),
            event(FAST_WORKSPACE, 3),
            event(SLOW_WORKSPACE, 1),
        ],
        "every answer is delivered once, the held one last"
    );

    token.cancel();
    lanes.drain().await;
}

/// Answers to one destination leave in the order they were queued, however
/// long each takes.
#[tokio::test(flavor = "multi_thread")]
async fn answers_to_one_destination_leave_in_the_order_they_were_queued() {
    let token = CancellationToken::new();
    let (lanes, poster) = lanes_against(&token);

    let count = 12_u32;
    for n in 1..=count {
        lanes.dispatch(job(FAST_WORKSPACE, n)).await;
    }
    await_until("every answer to be delivered", || {
        poster.delivered().len() == count as usize
    })
    .await;

    let expected: Vec<Uuid7> = (1..=count).map(|n| event(FAST_WORKSPACE, n)).collect();
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
    let token = CancellationToken::new();
    let (lanes, poster) = lanes_against(&token);

    let destinations = 5 * IN_FLIGHT_DELIVERIES;
    for n in 0..destinations {
        lanes
            .dispatch(job(n, u32::try_from(n).expect("small")))
            .await;
    }
    await_until("every destination's answer to be delivered", || {
        poster.delivered().len() == destinations
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
    let token = CancellationToken::new();
    let (lanes, poster) = lanes_against(&token);

    lanes.dispatch(job(FAST_WORKSPACE, 1)).await;
    await_until("the first answer to be delivered", || {
        poster.delivered().len() == 1
    })
    .await;
    await_until("the lane to retire", || lanes.active() == 0).await;

    lanes.dispatch(job(FAST_WORKSPACE, 2)).await;
    await_until("the second answer to be delivered", || {
        poster.delivered().len() == 2
    })
    .await;
    assert_eq!(
        poster.delivered(),
        vec![event(FAST_WORKSPACE, 1), event(FAST_WORKSPACE, 2)],
        "the fresh lane delivered after the retired one"
    );

    token.cancel();
    lanes.drain().await;
}

/// Cancellation finishes the delivery in hand; the lane starts nothing after
/// it. The job it takes next gives its claim back and the one queued behind
/// keeps its claim until it lapses, so either way another holder delivers
/// them, never this one.
#[tokio::test(flavor = "multi_thread")]
async fn cancellation_finishes_the_job_in_hand_and_starts_nothing_else() {
    let token = CancellationToken::new();
    let (lanes, poster) = lanes_against(&token);
    let gate = poster.shut(&workspace(SLOW_WORKSPACE));

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
        vec![event(SLOW_WORKSPACE, 1)],
        "the delivery in hand finished; nothing queued behind it was started"
    );
}

/// A job that lands while its lane is retiring is delivered IN ORDER with the
/// one dispatched behind it.
///
/// A retired lane returns the job for re-dispatch. The worker dispatches
/// serially, so that retry finishes before the next answer is queued. Jobs
/// published before retirement are observed under the shared entry guard.
///
/// Pairs dispatched back to back with no wait between them, each pair landing
/// while the previous pair's lane retires: the window is two statements wide,
/// so reaching it means aiming at it repeatedly rather than once.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_job_rescued_from_a_retiring_lane_is_not_overtaken() {
    let token = CancellationToken::new();
    let (lanes, poster) = lanes_against(&token);

    let rounds = 60_u32;
    for round in 0..rounds {
        lanes.dispatch(job(FAST_WORKSPACE, round * 2 + 1)).await;
        lanes.dispatch(job(FAST_WORKSPACE, round * 2 + 2)).await;
        let delivered = (round as usize + 1) * 2;
        await_until("the pair to be delivered", || {
            poster.delivered().len() == delivered
        })
        .await;
    }

    let expected: Vec<Uuid7> = (1..=rounds * 2).map(|n| event(FAST_WORKSPACE, n)).collect();
    assert_eq!(
        poster.delivered(),
        expected,
        "an answer rescued from a retiring lane was overtaken by the one behind it"
    );

    token.cancel();
    lanes.drain().await;
}
