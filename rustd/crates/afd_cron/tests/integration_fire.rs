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
use tracing_subscriber::layer::SubscriberExt as _;

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
#[ignore = "needs the lane's Dragonfly"]
async fn a_verified_fire_reaches_the_stream_once() {
    let lane = CronLane::open().await;
    let fire = Fire::new(lane.admissions().await);
    let schedule = CronLane::token();

    let fired = fire
        .deliver(&schedule, &target(&lane), MESSAGE_ID)
        .await
        .expect("the lane's Dragonfly takes the append");

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
#[ignore = "needs the lane's Dragonfly"]
async fn the_schedulers_retry_is_claimed_by_the_first_attempt() {
    let lane = CronLane::open().await;
    let fire = Fire::new(lane.admissions().await);
    let schedule = CronLane::token();

    let first = fire
        .deliver(&schedule, &target(&lane), MESSAGE_ID)
        .await
        .expect("the lane's Dragonfly takes the append");
    let retry = fire
        .deliver(&schedule, &target(&lane), MESSAGE_ID)
        .await
        .expect("the lane's Dragonfly answers the second attempt");

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
#[ignore = "needs the lane's Dragonfly"]
async fn two_daemons_receiving_one_retry_together_append_once() {
    let lane = CronLane::open().await;
    let target = target(&lane);
    let schedule = CronLane::token();

    // Two independent connections, as two processes would have.
    let left = Fire::new(lane.admissions().await);
    let right = Fire::new(lane.admissions().await);

    let (one, two) = tokio::join!(
        left.deliver(&schedule, &target, MESSAGE_ID),
        right.deliver(&schedule, &target, MESSAGE_ID),
    );
    let one = one.expect("the lane's Dragonfly answers the first daemon");
    let two = two.expect("the lane's Dragonfly answers the second daemon");

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
#[ignore = "needs the lane's Dragonfly"]
async fn two_schedules_firing_on_one_tick_do_not_silence_each_other() {
    let lane = CronLane::open().await;
    let fire = Fire::new(lane.admissions().await);
    let target = target(&lane);
    let nightly = CronLane::token();
    let hourly = CronLane::token();

    let first = fire
        .deliver(&nightly, &target, MESSAGE_ID)
        .await
        .expect("the lane's Dragonfly takes the append");
    let second = fire
        .deliver(&hourly, &target, MESSAGE_ID)
        .await
        .expect("the lane's Dragonfly takes the append");

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
#[ignore = "needs the lane's Dragonfly"]
async fn the_next_tick_of_one_schedule_is_a_new_fire() {
    let lane = CronLane::open().await;
    let fire = Fire::new(lane.admissions().await);
    let target = target(&lane);
    let schedule = CronLane::token();

    let tonight = fire
        .deliver(&schedule, &target, MESSAGE_ID)
        .await
        .expect("the lane's Dragonfly takes the append");
    let tomorrow = fire
        .deliver(&schedule, &target, "msg_01J8ZQ4X7K2P")
        .await
        .expect("the lane's Dragonfly takes the append");

    assert!(!tonight.replayed);
    assert!(
        !tomorrow.replayed,
        "suppressing this would skip a run the operator asked for"
    );
    assert_ne!(tonight.event_id, tomorrow.event_id);
}

/// Two fleets cannot claim over each other, even on one schedule id.
#[tokio::test]
#[ignore = "needs the lane's Dragonfly"]
async fn one_fleets_fire_does_not_claim_anothers() {
    let lane = CronLane::open().await;
    let other = CronLane::open().await;
    let fire = Fire::new(lane.admissions().await);
    let schedule = CronLane::token();

    let mine = fire
        .deliver(&schedule, &target(&lane), MESSAGE_ID)
        .await
        .expect("the lane's Dragonfly takes the append");
    let theirs = fire
        .deliver(&schedule, &target(&other), MESSAGE_ID)
        .await
        .expect("the lane's Dragonfly takes the append");

    assert!(!mine.replayed);
    assert!(
        !theirs.replayed,
        "the claim is scoped by fleet, so one tenant cannot suppress another's fire"
    );
}

/// The fire's log line names the workspace and the event it appended, so an
/// operator can find the run a schedule started.
#[tokio::test]
#[ignore = "needs the lane's Dragonfly"]
async fn a_fire_logs_the_workspace_and_event_it_appended() {
    let lane = CronLane::open().await;
    let fire = Fire::new(lane.admissions().await);
    let schedule = CronLane::token();
    let fields = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

    let fired = {
        let _scoped = tracing::subscriber::set_default(
            tracing_subscriber::registry().with(FireLog(std::sync::Arc::clone(&fields))),
        );
        fire.deliver(&schedule, &target(&lane), "msg_logged")
            .await
            .expect("the lane's Dragonfly takes the append")
    };

    let logged = fields
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    let appended = logged
        .iter()
        .find(|line| line.get(FIELD_EVENT).map(String::as_str) == Some(EVENT_APPENDED))
        .expect("the fire logs the append it made");
    assert_eq!(appended.get("workspace_id"), Some(&lane.workspace));
    assert_eq!(appended.get("event_id"), Some(&fired.event_id));
}

/// The log field every event names its kind under.
const FIELD_EVENT: &str = "event";

/// The event a fire logs once its append lands.
const EVENT_APPENDED: &str = "schedule_fire_appended";

/// Records every event's fields as text, under a subscriber scoped to one test.
struct FireLog(std::sync::Arc<std::sync::Mutex<Vec<std::collections::HashMap<String, String>>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for FireLog {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut fields = FieldText(std::collections::HashMap::new());
        event.record(&mut fields);
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(fields.0);
    }
}

/// One event's fields, as text.
struct FieldText(std::collections::HashMap<String, String>);

impl tracing::field::Visit for FieldText {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.0.insert(field.name().to_owned(), value.to_owned());
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.0.insert(field.name().to_owned(), format!("{value:?}"));
    }
}
