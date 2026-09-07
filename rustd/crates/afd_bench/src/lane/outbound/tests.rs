//! How destinations are scripted from the requested fractions.

#![expect(
    clippy::indexing_slicing,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use core::time::Duration;
use std::collections::BTreeMap;

use afd_outbound::Deliver as _;
use afd_redis::{EventId, OutboundDelivery};

use super::poster::{Behaviour, Scripted};
use super::record::window_end;
use super::{DESTINATIONS, Parameters, fraction_of, script};
use crate::fixture::RunPrefix;

fn parameters(slow: f64, retryable: f64) -> Parameters {
    Parameters {
        jobs: 1,
        slow_fraction: slow,
        retryable_fraction: retryable,
        window: Duration::from_secs(1),
    }
}

#[test]
fn test_one_in_sixteen_is_exactly_one_slow_destination() {
    let scripted = script(&RunPrefix::mint(), parameters(0.0625, 0.0));

    let slow = scripted.values().filter(|b| **b == Behaviour::Slow).count();
    assert_eq!(slow, 1);
    assert_eq!(
        scripted.len() as u64,
        DESTINATIONS,
        "every destination is scripted"
    );
}

#[test]
fn test_slow_and_retryable_populations_do_not_overlap() {
    let scripted = script(&RunPrefix::mint(), parameters(0.25, 0.25));

    let slow = scripted.values().filter(|b| **b == Behaviour::Slow).count();
    let retryable = scripted
        .values()
        .filter(|b| **b == Behaviour::Retryable)
        .count();
    let fast = scripted.values().filter(|b| **b == Behaviour::Fast).count();
    assert_eq!((slow, retryable, fast), (4, 4, 8));
}

#[test]
fn test_a_fraction_past_one_is_clamped_and_below_zero_selects_nothing() {
    assert_eq!(fraction_of(DESTINATIONS, 7.0), DESTINATIONS);
    assert_eq!(fraction_of(DESTINATIONS, -1.0), 0);
    assert_eq!(
        fraction_of(DESTINATIONS, f64::NAN),
        0,
        "NaN clamps to the floor, not to a panic"
    );
}

#[test]
fn test_a_fraction_that_selects_less_than_one_destination_selects_none() {
    assert_eq!(
        fraction_of(DESTINATIONS, 0.01),
        0,
        "rounded down, so a run does not get a slow destination it did not ask for"
    );
}

#[test]
fn test_two_scripts_of_one_prefix_deal_destinations_in_the_same_order() {
    let prefix = RunPrefix::mint();
    let first: Vec<String> = script(&prefix, parameters(0.25, 0.0)).into_keys().collect();
    let second: Vec<String> = script(&prefix, parameters(0.25, 0.0)).into_keys().collect();
    assert_eq!(
        first, second,
        "the interleaving is a function of the parameters, not a hash seed"
    );
    assert!(
        first.windows(2).all(|pair| pair[0] < pair[1]),
        "and it is sorted"
    );
}

/// The window closes when the last answer lands, not when the worker asked.
///
/// The first version stamped the attempt and counted it settled before the
/// scripted delay elapsed, so the slow lane's denominator lost its last 250 ms.
/// The delay here is real and short: the equality below is exact whatever the
/// clock did, and the count is read on either side of the await.
#[tokio::test]
async fn test_the_window_ends_when_the_last_answer_lands_not_when_it_is_asked_for() {
    let slow = Duration::from_millis(20);
    let mut behaviours = BTreeMap::new();
    behaviours.insert("slow".to_owned(), Behaviour::Slow);
    let poster = Scripted::new(behaviours, Duration::ZERO, slow);
    let job = OutboundDelivery {
        id: EventId::of("1-0"),
        provider: "slack".to_owned(),
        workspace_id: "w".to_owned(),
        fleet_id: "slow".to_owned(),
        event_id: "e".to_owned(),
        answer: "a".to_owned(),
    };

    let answer = poster.deliver(&job);
    assert_eq!(poster.settled(), 0, "asked is not yet answered");
    answer.await;
    assert_eq!(poster.settled(), 1, "the answer is what settles it");

    let seen = poster.seen();
    let asked = seen.attempts()["1-0"][0].at;
    assert_eq!(
        window_end(&seen, asked).saturating_duration_since(asked),
        slow,
        "the window closes when the slow answer lands"
    );
}
