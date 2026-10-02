//! The steer actor's one spelling, read back by the side that parses it.

use super::{ACTOR_MACHINE, is_steer_actor, steer_actor};

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
