//! How destinations are scripted from the requested fractions.

#![expect(
    clippy::indexing_slicing,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use core::time::Duration;

use super::poster::Behaviour;
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
