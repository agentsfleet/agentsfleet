//! The scheduler's answer to a push, read as the reconciler records it.

#![allow(
    clippy::expect_used,
    reason = "test target: an answer the case expects to be read should fail the test loudly"
)]

use super::confirmed;
use crate::error::detail::UPSTREAM_UNAVAILABLE;

/// The id a push sent, as `upstream_id` writes one.
const SENT: &str = "fleet-schedule-019329c5-0000-7000-8000-0000000000de";

/// An answer naming `id`, as the scheduler writes one.
fn answer(id: &str) -> String {
    format!(r#"{{"scheduleId":"{id}"}}"#)
}

#[test]
fn an_answer_naming_the_sent_id_is_the_registration() {
    let registered = confirmed(SENT.to_owned(), &answer(SENT))
        .expect("the scheduler filed the schedule under the id sent");

    assert_eq!(registered.schedule_id, SENT);
}

#[test]
fn an_answer_naming_an_id_the_scheduler_chose_is_refused() {
    // What a push whose id header never reached the scheduler gets back.
    let refused = confirmed(SENT.to_owned(), &answer("scd_62RMNMs9teSq1WGniPfFHCTKUyg7"))
        .expect_err("an id the scheduler chose must not be recorded as this schedule's");

    assert_eq!(refused.detail(), UPSTREAM_UNAVAILABLE);
    assert!(
        !refused.is_retryable(),
        "the scheduler answered; sending the same push again cannot change its answer"
    );
}

#[test]
fn an_answer_naming_another_schedule_of_this_daemon_is_refused() {
    let other = "fleet-schedule-019329c5-0000-7000-8000-0000000000df";

    confirmed(SENT.to_owned(), &answer(other))
        .expect_err("another schedule's id must not be recorded as this one's");
}

#[test]
fn an_answer_that_is_not_the_scheduler_shape_is_refused() {
    confirmed(SENT.to_owned(), "<html>gateway</html>")
        .expect_err("a page that is not JSON confirms nothing");
    confirmed(SENT.to_owned(), r#"{"id":"fleet-schedule-x"}"#)
        .expect_err("JSON without the scheduler's id field confirms nothing");
}
