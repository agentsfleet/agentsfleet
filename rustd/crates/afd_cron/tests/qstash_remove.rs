//! What a removal makes of the scheduler's answer.
//!
//! A 404 is the schedule already gone, which is what the caller wanted; any
//! other refusal leaves it registered and must not read as done, or a paused
//! schedule would go on firing with the row saying it stopped.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "support/stub_scheduler.rs"]
mod stub_scheduler;

use afd_cron::error::detail::UPSTREAM_UNAVAILABLE;

use self::stub_scheduler::StubScheduler;

/// The id a removal names, as `upstream_id` derives one.
const REGISTERED: &str = "fleet-schedule-019329c5-0000-7000-8000-0000000000de";

#[tokio::test]
async fn a_scheduler_refusing_the_removal_is_an_error_that_waits_for_sync() {
    let mut scheduler = StubScheduler::start();
    scheduler.answer(503);

    let refused = scheduler
        .client()
        .remove(REGISTERED)
        .await
        .expect_err("a refused removal leaves the schedule registered");

    assert_eq!(
        scheduler.arrived().await,
        format!("DELETE /schedules/{REGISTERED} HTTP/1.1")
    );
    assert_eq!(refused.detail(), UPSTREAM_UNAVAILABLE);
    assert!(
        !refused.is_retryable(),
        "the scheduler answered; the same request gets the same answer"
    );
}

#[tokio::test]
async fn a_scheduler_without_the_schedule_has_already_removed_it() {
    let scheduler = StubScheduler::start();
    scheduler.answer(404);

    scheduler
        .client()
        .remove(REGISTERED)
        .await
        .expect("a schedule already gone has met the caller's goal");
}
