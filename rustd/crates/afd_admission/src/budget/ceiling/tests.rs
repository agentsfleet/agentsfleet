//! What the sampled figure promises, and the race it is shaped around.

use super::{Ceiling, REFUSING_INTERVAL, SAMPLE_EVERY, SAMPLE_INTERVAL};
use afd_core::clock::UnixMillis;

/// A budget deep enough that only the tests that mean to cross it do.
const BUDGET: u64 = 100_000;

/// An arbitrary instant a figure is stamped at, far from the epoch so that
/// subtracting an interval from it stays positive.
const READ_AT: i64 = 1_700_000_000_000;

fn at(offset_millis: i64) -> UnixMillis {
    UnixMillis::from_millis(READ_AT.saturating_add(offset_millis))
}

/// A figure stamped at [`READ_AT`], so the interval tests have somewhere to
/// measure from.
fn sampled(rows: u64) -> Ceiling {
    let ceiling = Ceiling::default();
    ceiling.claim().publish(rows, at(0));
    ceiling
}

#[test]
fn an_unsampled_ceiling_is_due_however_recently_the_process_started() {
    let ceiling = Ceiling::default();
    assert_eq!(
        ceiling.estimate(),
        0,
        "nothing is counted before the first read"
    );
    assert!(
        ceiling.due(at(0), 0, BUDGET),
        "the first admission of a process has to take the first sample"
    );
    assert!(
        ceiling.due(at(i64::MIN), 0, BUDGET),
        "no clock reading makes an unread figure look fresh"
    );
}

#[test]
fn the_estimate_is_the_sample_plus_what_was_admitted_after_it() {
    let ceiling = sampled(100);
    assert_eq!(ceiling.estimate(), 100);
    ceiling.admitted();
    ceiling.admitted();
    assert_eq!(
        ceiling.estimate(),
        102,
        "two admissions after the sample are two rows the sample cannot know about"
    );
}

#[test]
fn a_published_figure_keeps_counting_admissions_that_raced_the_read() {
    let ceiling = sampled(0);
    let claim = ceiling.claim();
    // Three producers commit while the figure is in flight. The figure may
    // predate all three, so none of them may be taken off with it.
    ceiling.admitted();
    ceiling.admitted();
    ceiling.admitted();
    claim.publish(10, at(1));
    assert_eq!(
        ceiling.estimate(),
        13,
        "a figure of 10 read before three racing admissions still owes all three"
    );
}

#[test]
fn a_published_figure_takes_off_the_admissions_it_already_counted() {
    let ceiling = sampled(0);
    ceiling.admitted();
    ceiling.admitted();
    let claim = ceiling.claim();
    claim.publish(10, at(1));
    assert_eq!(
        ceiling.estimate(),
        10,
        "admissions committed before the read are in the figure, not beside it"
    );
}

#[test]
fn a_second_claim_while_one_is_held_does_not_win() {
    let ceiling = sampled(0);
    let held = ceiling.claim();
    assert!(held.won(), "an unheld ceiling hands out its first claim");
    let contender = ceiling.claim();
    assert!(
        !contender.won(),
        "two tasks sampling at once would publish two figures over each other"
    );
}

#[test]
fn a_dropped_claim_frees_the_right_to_sample() {
    let ceiling = sampled(0);
    drop(ceiling.claim());
    assert!(
        ceiling.claim().won(),
        "a failed read must not park the figure behind a claim nobody holds"
    );
}

#[test]
fn a_refusing_figure_is_due_sooner_than_a_healthy_one() {
    let ceiling = sampled(BUDGET);
    let just_past_refusing = i64::try_from(REFUSING_INTERVAL.as_millis()).unwrap_or(i64::MAX);
    assert!(
        ceiling.due(at(just_past_refusing), BUDGET, BUDGET),
        "a refusal clears only on a fresh figure, so it resamples on the short interval"
    );
    assert!(
        !ceiling.due(at(just_past_refusing), 0, BUDGET),
        "a deployment inside its budget waits the ordinary interval"
    );
    let ordinary = i64::try_from(SAMPLE_INTERVAL.as_millis()).unwrap_or(i64::MAX);
    assert!(
        ceiling.due(at(ordinary), 0, BUDGET),
        "the ordinary interval still bounds how far the figure trails a sibling replica"
    );
}

#[test]
fn the_admission_divisor_makes_a_fresh_figure_due() {
    let ceiling = sampled(0);
    for _ in 0..SAMPLE_EVERY {
        ceiling.admitted();
    }
    assert!(
        ceiling.due(at(1), SAMPLE_EVERY, BUDGET),
        "the counter is what bounds the figure when the clock has barely moved"
    );
}

#[test]
fn a_figure_stamped_in_the_future_is_not_due() {
    let ceiling = sampled(0);
    assert!(
        !ceiling.due(at(-60_000), 0, BUDGET),
        "a clock that went backwards must not be read as an infinitely old figure"
    );
}

#[test]
fn a_saturated_estimate_does_not_wrap_into_a_small_number() {
    let ceiling = sampled(u64::MAX);
    ceiling.admitted();
    assert_eq!(
        ceiling.estimate(),
        u64::MAX,
        "wrapping here would read a full deployment as an empty one and admit it"
    );
}
