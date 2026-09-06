//! What a knob reads as, and what it refuses.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use std::collections::HashMap;

use super::{fraction, number, required, variable};

fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + use<> {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect();
    move |key: &str| map.get(key).cloned()
}

#[test]
fn test_an_unset_number_is_its_default() {
    assert_eq!(
        number(&env_of(&[]), "BENCH_FLEETS", 200).expect("default"),
        200
    );
}

#[test]
fn test_a_set_number_is_read_with_surrounding_whitespace_ignored() {
    assert_eq!(
        number(&env_of(&[("BENCH_FLEETS", " 40 ")]), "BENCH_FLEETS", 200).expect("reads"),
        40
    );
}

#[test]
fn test_a_number_that_will_not_parse_is_refused_rather_than_defaulted() {
    let refused = number(&env_of(&[("BENCH_FLEETS", "1O00")]), "BENCH_FLEETS", 200)
        .expect_err("a letter O is not a thousand fleets");

    let message = refused.to_string();
    assert!(
        message.contains("BENCH_FLEETS") && message.contains("1O00"),
        "names the knob and its value: {message}"
    );
    assert!(refused.is_pre_flight());
}

#[test]
fn test_a_negative_number_is_refused() {
    number(&env_of(&[("BENCH_RUNNERS", "-4")]), "BENCH_RUNNERS", 8)
        .expect_err("no negative runner count");
}

#[test]
fn test_a_blank_variable_reads_as_unset() {
    assert_eq!(
        variable(&env_of(&[("BENCH_TARGET", "   ")]), "BENCH_TARGET"),
        None
    );
    assert_eq!(
        number(&env_of(&[("BENCH_JOBS", "")]), "BENCH_JOBS", 7).expect("blank is unset"),
        7
    );
}

#[test]
fn test_a_required_variable_that_is_unset_is_refused_by_name() {
    let refused = required(&env_of(&[]), "BENCH_REDIS_URL").expect_err("no guessing a datastore");
    assert!(refused.to_string().contains("BENCH_REDIS_URL"));
}

#[test]
fn test_a_fraction_outside_the_unit_interval_is_refused() {
    fraction(
        &env_of(&[("BENCH_SLOW_FRACTION", "1.5")]),
        "BENCH_SLOW_FRACTION",
        0.0,
    )
    .expect_err("more than everything");
    fraction(
        &env_of(&[("BENCH_SLOW_FRACTION", "-0.1")]),
        "BENCH_SLOW_FRACTION",
        0.0,
    )
    .expect_err("less than nothing");
    fraction(
        &env_of(&[("BENCH_SLOW_FRACTION", "half")]),
        "BENCH_SLOW_FRACTION",
        0.0,
    )
    .expect_err("not a number");
}

#[test]
fn test_a_fraction_at_either_bound_is_allowed() {
    assert!((fraction(&env_of(&[("F", "0")]), "F", 0.5).expect("zero") - 0.0).abs() < f64::EPSILON);
    assert!((fraction(&env_of(&[("F", "1")]), "F", 0.5).expect("one") - 1.0).abs() < f64::EPSILON);
}
