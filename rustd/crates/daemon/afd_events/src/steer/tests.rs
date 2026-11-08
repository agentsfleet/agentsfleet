//! The steer actor's one spelling, read back by the side that parses it, and
//! what a capacity refusal answers once the repeat lookup has run.
#![expect(
    clippy::expect_used,
    reason = "test module: a missing fixture should fail the test loudly"
)]

use super::{ACTOR_MACHINE, despite, is_steer_actor, steer_actor};
use crate::error::operation_conflict;

/// A subject an identity provider hands out.
const SUBJECT: &str = "user_2abc";

/// The actor is the prefix then the subject, byte for byte: the onboarding
/// read matches `steer:%` and the members list must name the same string.
#[test]
fn should_record_a_person_as_the_prefix_then_their_subject() {
    // pin test: literal is the contract
    assert_eq!(steer_actor(SUBJECT), "steer:user_2abc");
}

/// What one side writes the other side reads as a steer, a machine's included.
#[test]
fn should_read_both_steer_actors_as_steers() {
    assert!(is_steer_actor(&steer_actor(SUBJECT)));
    assert!(is_steer_actor(ACTOR_MACHINE));
}

/// Every other producer's actor is not a steer, so its body is never read as
/// typed words.
#[test]
fn should_not_read_another_producers_actor_as_a_steer() {
    for actor in ["webhook:github", "continuation:01", "cron", ""] {
        assert!(!is_steer_actor(actor), "{actor} is not a steer");
    }
}

/// A budget refusal, as the admission ledger raises one.
fn over_capacity() -> afd_admission::Error {
    afd_admission::error::one_of_each_kind()
        .into_iter()
        .find_map(|(_, error)| error.is_over_capacity().then_some(error))
        .expect("the ledger has a capacity refusal")
}

/// A repeat found after the budget refused answers with the first event.
#[test]
fn should_answer_a_repeat_found_despite_the_budget_with_its_event() {
    let answered = despite(Ok(Some("event-1".to_owned())), over_capacity());

    assert_eq!(answered.ok().as_deref(), Some("event-1"));
}

/// The same id with a changed message is a conflict, not a retryable refusal.
#[test]
fn should_refuse_a_drifted_repeat_as_a_conflict_despite_the_budget() {
    let answered = despite(Err(operation_conflict()), over_capacity());

    assert!(answered.is_err_and(|error| error.is_operation_conflict()));
}

/// New work, and a lookup that failed too, keep the capacity refusal.
#[test]
fn should_keep_the_capacity_refusal_for_new_work_or_a_failed_lookup() {
    for repeat in [Ok(None), Err(over_capacity().into())] {
        let answered = despite(repeat, over_capacity());

        assert!(answered.is_err_and(|error| error.is_datastore_unavailable()));
    }
}
