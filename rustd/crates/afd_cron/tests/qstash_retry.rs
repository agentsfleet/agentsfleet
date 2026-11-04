//! Which scheduler failures a push or a removal is asked again for.
//!
//! A scheduler that could not be reached, a 429 and a 5xx can heal on their
//! own, so each is repeated on a short jittered schedule before the row is
//! marked failed. Any other answer is final the first time. Every case queues
//! one answer more than it expects to be asked for, so a repeat that should not
//! happen shows up as a different outcome rather than as a hang.

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "support/stub_scheduler.rs"]
mod stub_scheduler;

use std::time::{Duration, Instant};

use afd_core::id::Uuid7;
use afd_cron::Attempts;
use afd_cron::error::detail::UPSTREAM_UNAVAILABLE;
use afd_cron::qstash::{QStash, Registered};
use afd_cron::upstream_id;

use self::stub_scheduler::{StubScheduler, client_at, unreachable_client};

/// The schedule every push names.
const SCHEDULE: &str = "019329c5-0000-7000-8000-0000000000de";

/// The shortest the two waits between three attempts can add up to: the
/// first delay is at least 200ms and the second at least 400ms.
const LEAST_WAIT: Duration = Duration::from_millis(600);

fn schedule() -> Uuid7 {
    Uuid7::parse(SCHEDULE).expect("a canonical identifier")
}

/// What the scheduler answers a push it filed under the id sent.
fn filed() -> String {
    format!(r#"{{"scheduleId":"{}"}}"#, upstream_id(&schedule()))
}

async fn push(client: &QStash) -> afd_cron::Result<Registered> {
    client
        .upsert(
            &schedule(),
            "0 9 * * 1-5",
            "UTC",
            "Summarise overnight tickets",
            Attempts::Retried,
        )
        .await
}

async fn remove(client: &QStash) -> afd_cron::Result<()> {
    client
        .remove(&upstream_id(&schedule()), Attempts::Retried)
        .await
}

#[tokio::test]
async fn a_push_the_scheduler_throttled_is_registered_on_the_repeat() {
    let mut scheduler = StubScheduler::start();
    scheduler.answer(429);
    scheduler.answer_with(200, &filed());

    let registered = push(&scheduler.client())
        .await
        .expect("a 429 is asked again, and the repeat registers");

    assert_eq!(registered.schedule_id, upstream_id(&schedule()));
    let arrived = scheduler.drained();
    let [first, repeat] = arrived.as_slice() else {
        panic!("asked twice: {arrived:?}");
    };
    assert!(first.starts_with("POST /schedules/"), "{first}");
    assert_eq!(repeat, first, "the repeat is the same push");
}

#[tokio::test]
async fn a_removal_the_scheduler_was_briefly_unable_to_answer_succeeds_on_the_repeat() {
    let mut scheduler = StubScheduler::start();
    scheduler.answer(503);
    scheduler.answer(200);

    remove(&scheduler.client())
        .await
        .expect("a 503 is asked again, and the repeat removes");

    let line = format!("DELETE /schedules/{} HTTP/1.1", upstream_id(&schedule()));
    assert_eq!(scheduler.drained(), [line.clone(), line]);
}

#[tokio::test]
async fn a_scheduler_failing_every_attempt_is_asked_three_times_then_reported() {
    let mut scheduler = StubScheduler::start();
    // Both ends of the 5xx range, and a fourth answer that would succeed.
    scheduler.answer(500);
    scheduler.answer(503);
    scheduler.answer(599);
    scheduler.answer(200);

    let refused = remove(&scheduler.client())
        .await
        .expect_err("three failed attempts are the whole budget");

    assert_eq!(refused.detail(), UPSTREAM_UNAVAILABLE);
    assert!(
        !refused.is_retryable(),
        "the scheduler answered; the row waits for the next sync"
    );
    let arrived = scheduler.drained();
    assert_eq!(arrived.len(), 3, "{arrived:?}");
    assert!(
        arrived.iter().all(|line| line.starts_with("DELETE ")),
        "{arrived:?}"
    );
}

#[tokio::test]
async fn a_push_the_scheduler_rejected_is_not_repeated() {
    let mut scheduler = StubScheduler::start();
    scheduler.answer(400);
    scheduler.answer_with(200, &filed());

    let refused = push(&scheduler.client())
        .await
        .expect_err("a 400 will be a 400 however often it is sent");

    assert_eq!(refused.detail(), UPSTREAM_UNAVAILABLE);
    assert!(!refused.is_retryable());
    assert_eq!(scheduler.drained().len(), 1, "asked once");
}

#[tokio::test]
async fn a_removal_refused_just_below_the_5xx_range_is_not_repeated() {
    let mut scheduler = StubScheduler::start();
    scheduler.answer(499);
    scheduler.answer(200);

    let refused = remove(&scheduler.client())
        .await
        .expect_err("a 4xx other than 404 and 429 is final");
    assert_eq!(refused.detail(), UPSTREAM_UNAVAILABLE);
    assert!(!refused.is_retryable());
    assert_eq!(scheduler.drained().len(), 1, "asked once");
}

#[tokio::test]
async fn a_push_answered_in_a_shape_the_daemon_cannot_read_is_not_repeated() {
    let mut scheduler = StubScheduler::start();
    scheduler.answer_with(200, "not the scheduler's shape");
    scheduler.answer_with(200, &filed());

    let refused = push(&scheduler.client())
        .await
        .expect_err("an unreadable answer reads the same way the second time");

    assert_eq!(refused.detail(), UPSTREAM_UNAVAILABLE);
    assert!(!refused.is_retryable());
    assert_eq!(scheduler.drained().len(), 1, "asked once");
}

#[tokio::test]
async fn a_scheduler_that_cannot_be_reached_is_tried_again_before_it_is_reported() {
    let started = Instant::now();

    let unreachable = push(&unreachable_client())
        .await
        .expect_err("nothing listens on the port");

    assert!(
        unreachable.is_retryable(),
        "a scheduler never reached is still owed the push"
    );
    assert_eq!(unreachable.detail(), UPSTREAM_UNAVAILABLE);
    assert!(
        started.elapsed() >= LEAST_WAIT,
        "two waits come before the push is reported: {:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn a_push_whose_answer_is_lost_is_repeated_under_the_same_schedule_id() {
    let mut scheduler = StubScheduler::start();
    // The scheduler may have filed the first push; its answer never arrives.
    scheduler.answer_lost();
    scheduler.answer_with(200, &filed());

    let registered = push(&scheduler.client())
        .await
        .expect("a lost answer is asked again");

    assert_eq!(registered.schedule_id, upstream_id(&schedule()));
    assert_eq!(scheduler.drained().len(), 2, "asked twice");
    // The repeat is safe only because it names the same schedule, so the
    // scheduler overwrites what the first push may have filed.
    let sent = Some(upstream_id(&schedule()));
    assert_eq!(scheduler.schedule_ids(), [sent.clone(), sent]);
}

#[tokio::test(start_paused = true)]
async fn a_scheduler_that_never_answers_is_given_up_on_inside_the_sync_fence() {
    // Takes every connection and never replies. Under the paused clock the
    // attempts' bounds fire before the accept loop runs, so this proves the
    // bound, not a connection count: without it the call would wait forever
    // and the outer lease timeout below would fail the test.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind a loopback port");
    let base = format!(
        "http://{}",
        listener.local_addr().expect("the bound address")
    );
    tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((stream, _peer)) = listener.accept().await {
            held.push(stream);
        }
    });
    let started = tokio::time::Instant::now();

    let hung = tokio::time::timeout(afd_cron::SYNC_LEASE, remove(&client_at(base)))
        .await
        .expect("every attempt carries its own bound, so the call ends inside the fence")
        .expect_err("no answer ever comes");

    assert!(
        hung.is_retryable(),
        "a scheduler that never answered is still owed the call"
    );
    assert_eq!(hung.detail(), UPSTREAM_UNAVAILABLE);
    assert!(
        started.elapsed() < afd_cron::SYNC_LEASE,
        "{:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn a_call_sent_once_is_not_repeated_even_when_it_could_heal() {
    let mut scheduler = StubScheduler::start();
    scheduler.answer(503);
    scheduler.answer(200);

    let refused = scheduler
        .client()
        .remove(&upstream_id(&schedule()), Attempts::Once)
        .await
        .expect_err("one attempt, and it was refused");

    assert_eq!(refused.detail(), UPSTREAM_UNAVAILABLE);
    assert_eq!(scheduler.drained().len(), 1, "asked once");
}
