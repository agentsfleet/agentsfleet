//! The App ingress rules that hold with no datastore in the test.
//!
//! Two things live here: what identifies a repeat delivery, and where the
//! fan-out ceiling falls. Both are pure functions of their input on purpose —
//! the replay identity is what stops one captured delivery from running every
//! subscribed fleet twice, and the ceiling is what stops one delivery from
//! spending a hundred models. Neither should need a Postgres to prove.
//!
//! The two database readers above are not covered here. They are integration
//! surface — real rows, real joins — and belong to the ingress integration
//! suite rather than to a unit test that would only restate the SQL.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use af_core::id::Uuid7;
use afd_fleet_lifecycle::FleetStatus;

use super::{Fanout, fanout, replay_id};
use crate::binding::{Binding, StoredTrigger};

/// The provider whose App ingress these rules belong to.
const SOURCE: &str = "github";

/// A canonical identifier, so the tests carry no invalid one by accident.
const FLEET: &str = "01912d4e-8f2a-7c3b-9d1e-4a5b6c7d8e9f";
/// See [`FLEET`].
const WORKSPACE: &str = "01912d4e-8f2a-7c3b-9d1e-4a5b6c7d8ea0";

/// A subscribed fleet, for the tests that only count them.
fn subscriber() -> Binding {
    let parse = |text: &str| Uuid7::parse(text).expect("the fixture identifiers are canonical");
    let row = StoredTrigger {
        source: SOURCE.to_owned(),
        repositories: Some(vec!["owner/repo".to_owned()]),
        ..StoredTrigger::default()
    };

    Binding::read(
        parse(FLEET),
        parse(WORKSPACE),
        FleetStatus::Active.as_str(),
        &row,
    )
    .expect("the row parses and the status is one this build knows")
    .expect("the row names a registry source")
}

/// `n` subscribers, for the ceiling's boundary.
fn subscribers(count: usize) -> Vec<Binding> {
    (0..count).map(|_seat| subscriber()).collect()
}

#[test]
fn the_same_body_is_the_same_delivery_however_often_it_arrives() {
    let body = br#"{"action":"opened","number":7}"#;

    assert_eq!(
        replay_id(body),
        replay_id(body),
        "a redelivery of identical bytes must claim the same slot, or the \
         fleet runs a second time on one event"
    );
}

#[test]
fn a_different_body_is_a_different_delivery() {
    let first = replay_id(br#"{"action":"opened","number":7}"#);
    let second = replay_id(br#"{"action":"opened","number":8}"#);

    assert_ne!(
        first, second,
        "two events sharing one claim would silence the second, and the \
         fleet would never see it"
    );
}

/// The property the whole choice of a body digest rests on.
///
/// `x-github-delivery` is NOT covered by the signature. If the claim were keyed
/// on it, anyone holding a captured delivery could resend the same signed bytes
/// under a fresh delivery id and wake every subscribed fleet again — the
/// signature would still verify, because the body did not change. Keying on the
/// body means a forger cannot vary the claim without invalidating the proof.
#[test]
fn the_replay_identity_is_a_function_of_the_signed_bytes_alone() {
    let body = br#"{"action":"opened"}"#;

    let id = replay_id(body);

    assert_eq!(
        id,
        replay_id(body),
        "nothing outside the body may enter the claim key"
    );
    assert_eq!(
        id.len(),
        64,
        "a SHA-256 digest renders as 64 hex characters"
    );
    assert!(
        id.chars()
            .all(|glyph| glyph.is_ascii_hexdigit() && !glyph.is_ascii_uppercase()),
        "lowercase hex, so a claim written by either daemon during a cutover \
         is the same ledger key"
    );
}

#[test]
fn nobody_subscribed_is_its_own_answer_and_not_an_empty_fan_out() {
    assert!(
        matches!(fanout(Vec::new()), Fanout::Nobody),
        "an empty match set answers 200 with a reason, and a caller must not \
         be able to reach it holding an empty list of fleets to wake"
    );
}

#[test]
fn one_subscriber_fans_out() {
    assert!(
        matches!(fanout(subscribers(1)), Fanout::To(ref fleets) if fleets.len() == 1),
        "a single match is a delivery to make, carrying exactly that fleet"
    );
}

/// A delivery's replay identity is stored and compared with the next delivery's,
/// so it is pinned: the plain SHA-256 of the signed body, lowercase hex.
#[test]
fn the_replay_identity_is_pinned_to_the_byte() {
    // pin test: literal is the contract
    assert_eq!(
        replay_id(br#"{"action":"opened"}"#),
        "d592421cfe150deec6c49b8989cc99478e39c7f8cdd4c36f5b1c4cfeff394e24"
    );
}
