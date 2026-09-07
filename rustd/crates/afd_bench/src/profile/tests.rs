//! What the profile refuses, and what it refuses it for.
//!
//! Every test here runs without a datastore, because every check under test
//! runs before a connection opens. A test needing one would mean the refusal
//! had moved too late.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use std::collections::HashMap;

use super::{
    ACKNOWLEDGEMENT_VALUE, ACKNOWLEDGEMENT_VARIABLE, Parameter, Profile, TARGET_RIG,
    TARGET_VARIABLE, Target,
};

/// An environment holding exactly what a test put in it.
fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + use<> {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect();
    move |key: &str| map.get(key).cloned()
}

/// An environment with nothing in it at all.
fn empty_env() -> impl Fn(&str) -> Option<String> + use<> {
    env_of(&[])
}

#[test]
fn test_a_parameter_above_the_profile_cap_is_refused() {
    let cap = Profile::Dev.caps().fleets;

    let refused = Profile::Dev
        .check(Parameter::Fleets, cap + 1)
        .expect_err("a fleet count above the dev cap must be refused");

    let message = refused.to_string();
    assert!(
        message.contains(&cap.to_string()),
        "the refusal must name the cap it enforced, said: {message}"
    );
    assert!(
        message.contains("dev"),
        "the refusal must name the profile that set the cap, said: {message}"
    );
    assert!(
        refused.is_pre_flight(),
        "a cap is checked before anything is opened"
    );
}

#[test]
fn test_a_parameter_at_the_cap_is_allowed() {
    let cap = Profile::Dev.caps().fleets;

    Profile::Dev
        .check(Parameter::Fleets, cap)
        .expect("the cap is a ceiling, not the first refused value");
}

#[test]
fn test_production_requires_an_explicit_acknowledgement() {
    let refused = Profile::Prod
        .acknowledge(&empty_env())
        .expect_err("prod without its acknowledgement must refuse");

    let message = refused.to_string();
    assert!(
        message.contains(ACKNOWLEDGEMENT_VARIABLE),
        "the refusal must name the variable it needs, said: {message}"
    );
    assert!(
        refused.is_pre_flight(),
        "the acknowledgement is checked before anything is opened"
    );
}

#[test]
fn test_an_acknowledgement_with_the_wrong_value_is_still_a_refusal() {
    let present_but_wrong = env_of(&[(ACKNOWLEDGEMENT_VARIABLE, "1")]);

    Profile::Prod
        .acknowledge(&present_but_wrong)
        .expect_err("a truthy-looking value is not the phrase the gate asks for");
}

#[test]
fn test_the_acknowledgement_admits_production_when_spoken_exactly() {
    let spoken = env_of(&[
        (ACKNOWLEDGEMENT_VARIABLE, ACKNOWLEDGEMENT_VALUE),
        (TARGET_VARIABLE, TARGET_RIG),
    ]);

    let target = Profile::Prod
        .admit(&spoken)
        .expect("the phrase spoken exactly admits the run");

    assert_eq!(target, Target::Rig);
}

#[test]
fn test_only_production_asks_for_an_acknowledgement() {
    for profile in [Profile::Rig, Profile::Dev] {
        profile
            .acknowledge(&empty_env())
            .expect("only prod carries the acknowledgement gate");
    }
}

#[test]
fn test_a_deployed_profile_without_a_target_is_refused() {
    let refused = Profile::Dev
        .target(&empty_env())
        .expect_err("a deployed profile with nowhere to point must refuse");

    let message = refused.to_string();
    assert!(
        message.contains(TARGET_VARIABLE),
        "the refusal must name the variable that would supply a target, said: {message}"
    );
}

#[test]
fn test_a_deployed_profile_can_point_at_the_rig() {
    let pointed_at_rig = env_of(&[(TARGET_VARIABLE, TARGET_RIG)]);

    let target = Profile::Dev
        .target(&pointed_at_rig)
        .expect("dev semantics against the compose rig is a supported run");

    assert_eq!(
        target,
        Target::Rig,
        "the rig token selects the compose datastores, not an address literally called rig"
    );
}

#[test]
fn test_a_deployed_profile_keeps_the_address_it_was_given() {
    let address = "https://dev.example.invalid";
    let pointed_away = env_of(&[(TARGET_VARIABLE, address)]);

    let target = Profile::Dev
        .target(&pointed_away)
        .expect("an address is a valid target");

    assert_eq!(
        target,
        Target::Deployed {
            address: address.to_owned()
        }
    );
}

#[test]
fn test_the_rig_ignores_the_target_variable_entirely() {
    let pointed_elsewhere = env_of(&[(TARGET_VARIABLE, "https://prod.example.invalid")]);

    let target = Profile::Rig
        .target(&pointed_elsewhere)
        .expect("the rig is always the compose datastores");

    assert_eq!(
        target,
        Target::Rig,
        "a stray target variable must not be able to aim the rig profile at a deployment"
    );
}

#[test]
fn test_an_unknown_profile_name_is_refused_rather_than_defaulted() {
    let refused = "staging"
        .parse::<Profile>()
        .expect_err("an unrecognised profile must not default to one");

    let message = refused.to_string();
    assert!(
        message.contains("staging"),
        "the refusal must quote what was asked for, said: {message}"
    );
}

#[test]
fn test_every_profile_round_trips_through_its_name() {
    for profile in [Profile::Rig, Profile::Dev, Profile::Prod] {
        let parsed: Profile = profile
            .to_string()
            .parse()
            .expect("a profile's own name must parse back to it");
        assert_eq!(parsed, profile);
    }
}

#[test]
fn test_the_deployed_profiles_are_capped_below_the_rig() {
    let rig = Profile::Rig.caps();
    let dev = Profile::Dev.caps();
    let prod = Profile::Prod.caps();

    assert!(prod.fleets < dev.fleets && dev.fleets < rig.fleets);
    assert!(prod.tasks < dev.tasks && dev.tasks < rig.tasks);
    assert!(
        prod.abort_error_rate < dev.abort_error_rate && dev.abort_error_rate < rig.abort_error_rate,
        "the smaller the blast radius allowed, the sooner a failing target stops the run"
    );
}

#[test]
fn test_every_parameter_is_checked_against_a_ceiling() {
    let caps = Profile::Prod.caps();

    for parameter in [
        Parameter::Fleets,
        Parameter::Runners,
        Parameter::Jobs,
        Parameter::Concurrency,
    ] {
        let ceiling = caps.ceiling(parameter);
        assert!(
            ceiling > 0,
            "{} has no ceiling, so nothing would refuse it",
            parameter.name()
        );
        Profile::Prod
            .check(parameter, ceiling + 1)
            .expect_err("a value above the ceiling must be refused");
    }
}

#[test]
fn test_only_deployed_profiles_reach_over_the_network() {
    assert!(!Profile::Rig.is_deployed());
    assert!(Profile::Dev.is_deployed());
    assert!(Profile::Prod.is_deployed());
}

#[test]
fn test_production_is_told_about_the_acknowledgement_before_the_target() {
    let refused = Profile::Prod.admit(&empty_env()).expect_err("refuses");
    assert!(
        matches!(refused, crate::Error::AcknowledgementMissing { .. }),
        "someone pointing at production is told that first, got {refused}"
    );
}

#[test]
fn test_a_blank_target_is_unset_not_an_address_of_one_space() {
    let blank = env_of(&[(TARGET_VARIABLE, "   ")]);
    let refused = Profile::Dev.target(&blank).expect_err("blank is unset");
    assert!(
        matches!(refused, crate::Error::TargetMissing { .. }),
        "got {refused}"
    );
}

#[test]
fn test_zero_is_below_every_floor() {
    let refused = Profile::Rig
        .check(Parameter::Runners, 0)
        .expect_err("zero runners spawn nothing");
    assert!(
        refused.to_string().contains("BENCH_RUNNERS"),
        "names the knob: {refused}"
    );
}

#[test]
fn test_a_window_under_the_warmup_floor_is_refused() {
    let refused = Profile::Rig
        .check_window(core::time::Duration::from_millis(1))
        .expect_err("a rate across a cold cache is not a rate");
    assert!(matches!(refused, crate::Error::WindowTooShort { .. }));
    Profile::Rig
        .check_window(Profile::Rig.caps().warmup)
        .expect("the floor itself is allowed");
}
