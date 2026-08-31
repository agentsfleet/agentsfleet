//! A verified schedule fire reaches the stream exactly once, however often the
//! scheduler sends it.
//!
//! # The claim key is the scheduler's own message id, and it has to be
//!
//! The external scheduler retries a callback it did not get a 2xx for, and it
//! repeats its own message id when it does. That id is the only value that
//! identifies "this fire" across attempts: a key minted here would make every
//! retry a new fire — the duplicate run this exists to prevent — and a key
//! derived from the body's digest would collapse two genuinely separate fires
//! of the same schedule into one.
//!
//! # Concurrency is the point, not an edge case
//!
//! Two daemons behind one load balancer can receive the same retry at the same
//! moment. The claim and the append are one Lua script, so the second loses the
//! claim rather than appending — there is no window between "check" and "write"
//! for both to pass through. The concurrent case below is the one that would
//! still pass if the script were split into two commands and run slowly enough,
//! which is why it races them rather than sequencing them.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "support/cron_lane.rs"]
mod support;

use afd_cron::{DesiredStatus, Fire, FireTarget};

use self::support::CronLane;

/// The scheduler's own id for one delivery, repeated across its retries.
const MESSAGE_ID: &str = "msg_01J8ZQ4X7K2N";

/// What the fleet is asked to do when it fires.
const MESSAGE: &str = "run the nightly repair";

/// What one fire resolves to, for a lane's fleet.
fn target(lane: &CronLane) -> FireTarget {
    FireTarget {
        fleet: lane.fleet_id(),
        workspace: lane.workspace_id(),
        message: MESSAGE.to_owned(),
        desired_status: DesiredStatus::Active,
        fleet_status: "active".to_owned(),
    }
}

#[tokio::test]
#[ignore = "needs the lane's Redis"]
async fn a_verified_fire_reaches_the_stream_once() {
    let lane = CronLane::open().await;
    let fire = Fire::new(CronLane::queue().await);
    let schedule = CronLane::token();

    let fired = fire
        .deliver(&schedule, &target(&lane), MESSAGE_ID)
        .await
        .expect("the lane's Redis takes the append");

    assert!(
        !fired.replayed,
        "the first delivery of a fire is not a replay"
    );
    assert!(
        !fired.event_id.is_empty(),
        "the entry id is what the caller answers with"
    );
}

/// The retry case, which is the ordinary one rather than the exceptional one.
#[tokio::test]
#[ignore = "needs the lane's Redis"]
async fn the_schedulers_retry_is_claimed_by_the_first_attempt() {
    let lane = CronLane::open().await;
    let fire = Fire::new(CronLane::queue().await);
    let schedule = CronLane::token();

    let first = fire
        .deliver(&schedule, &target(&lane), MESSAGE_ID)
        .await
        .expect("the lane's Redis takes the append");
    let retry = fire
        .deliver(&schedule, &target(&lane), MESSAGE_ID)
        .await
        .expect("the lane's Redis answers the second attempt");

    assert!(!first.replayed);
    assert!(retry.replayed, "a repeated message id is the same fire");
    assert_eq!(
        retry.event_id, first.event_id,
        "the retry must be told the id the FIRST attempt wrote, or the caller \
         answers the scheduler with an entry that does not exist"
    );
}

/// Two daemons, one retry, at the same moment.
#[tokio::test]
#[ignore = "needs the lane's Redis"]
async fn two_daemons_receiving_one_retry_together_append_once() {
    let lane = CronLane::open().await;
    let target = target(&lane);
    let schedule = CronLane::token();

    // Two independent connections, as two processes would have.
    let left = Fire::new(CronLane::queue().await);
    let right = Fire::new(CronLane::queue().await);

    let (one, two) = tokio::join!(
        left.deliver(&schedule, &target, MESSAGE_ID),
        right.deliver(&schedule, &target, MESSAGE_ID),
    );
    let one = one.expect("the lane's Redis answers the first daemon");
    let two = two.expect("the lane's Redis answers the second daemon");

    assert_eq!(
        one.event_id, two.event_id,
        "both daemons must answer with the one entry that exists"
    );
    assert_eq!(
        usize::from(one.replayed) + usize::from(two.replayed),
        1,
        "exactly one of the two claimed the fire and one found it taken; \
         two claims means the check and the write came apart"
    );
}

/// The claim is scoped by SCHEDULE as well as by fleet.
///
/// One fleet may hold many schedules, and a key that was the message id alone
/// would let two schedules firing on the same tick silence each other — the
/// second would be reported as a replay and the fleet would never be woken for
/// it.
#[tokio::test]
#[ignore = "needs the lane's Redis"]
async fn two_schedules_firing_on_one_tick_do_not_silence_each_other() {
    let lane = CronLane::open().await;
    let fire = Fire::new(CronLane::queue().await);
    let target = target(&lane);
    let nightly = CronLane::token();
    let hourly = CronLane::token();

    let first = fire
        .deliver(&nightly, &target, MESSAGE_ID)
        .await
        .expect("the lane's Redis takes the append");
    let second = fire
        .deliver(&hourly, &target, MESSAGE_ID)
        .await
        .expect("the lane's Redis takes the append");

    assert!(!first.replayed);
    assert!(
        !second.replayed,
        "a different schedule is a different fire, even under the same message id"
    );
    assert_ne!(first.event_id, second.event_id, "two fires, two entries");
}

/// A second delivery of the same schedule under a new id is a new fire.
///
/// The scheduler repeats its id only for a RETRY. A fresh id means the schedule
/// came round again, and suppressing that would silently skip a run.
#[tokio::test]
#[ignore = "needs the lane's Redis"]
async fn the_next_tick_of_one_schedule_is_a_new_fire() {
    let lane = CronLane::open().await;
    let fire = Fire::new(CronLane::queue().await);
    let target = target(&lane);
    let schedule = CronLane::token();

    let tonight = fire
        .deliver(&schedule, &target, MESSAGE_ID)
        .await
        .expect("the lane's Redis takes the append");
    let tomorrow = fire
        .deliver(&schedule, &target, "msg_01J8ZQ4X7K2P")
        .await
        .expect("the lane's Redis takes the append");

    assert!(!tonight.replayed);
    assert!(
        !tomorrow.replayed,
        "suppressing this would skip a run the operator asked for"
    );
    assert_ne!(tonight.event_id, tomorrow.event_id);
}

/// Two fleets cannot claim over each other, even on one schedule id.
#[tokio::test]
#[ignore = "needs the lane's Redis"]
async fn one_fleets_fire_does_not_claim_anothers() {
    let lane = CronLane::open().await;
    let other = CronLane::open().await;
    let fire = Fire::new(CronLane::queue().await);
    let schedule = CronLane::token();

    let mine = fire
        .deliver(&schedule, &target(&lane), MESSAGE_ID)
        .await
        .expect("the lane's Redis takes the append");
    let theirs = fire
        .deliver(&schedule, &target(&other), MESSAGE_ID)
        .await
        .expect("the lane's Redis takes the append");

    assert!(!mine.replayed);
    assert!(
        !theirs.replayed,
        "the claim is scoped by fleet, so one tenant cannot suppress another's fire"
    );
}
