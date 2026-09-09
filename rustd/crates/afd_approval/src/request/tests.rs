//! What a request FOUND, mapped to what its caller does next.
//!
//! The Failure Mode this table exists for: [`super::settle`] is the only place
//! a grant row's status becomes an instruction, and one of its five answers —
//! [`Requested::Denied`] — ends a delivery permanently. Reading a status wrong
//! in that direction destroys work nobody refused, so every arm is pinned here
//! rather than left to the one integration test that happens to walk it.
//!
//! Nothing here needs a datastore: the mapping is a pure function of the
//! snapshot the statement returned.
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use afd_core::id::Uuid7;
use afd_wire::grant::status;

use super::{Origin, Requested, Wanted, settle};

/// A canonical v7 identifier, since `settle` only ever reads it for a log field.
fn fleet() -> Uuid7 {
    Uuid7::parse("01890a5d-ac96-774b-bcce-b302099a8057").expect("a valid v7 identifier")
}

/// `settle` over one status, at the card count the statement reported.
fn found(status: Option<&str>, raised: bool) -> Requested {
    settle(status, raised, &fleet(), "github")
}

#[test]
fn an_absent_grant_row_is_a_fresh_raise() {
    // The select shares the writes' snapshot, so the row this statement just
    // inserted reads as absent. That is what makes a first request `Raised`.
    assert_eq!(found(None, true), Requested::Raised);
}

#[test]
fn the_loser_of_a_concurrent_request_reports_the_question_that_stands() {
    // Slot 836's unique index sends the loser of a same-instant race through
    // `ON CONFLICT DO NOTHING`: it wrote no card, and the winner's row is not in
    // its snapshot, so it sees an absent grant and a zero count. `Raised` is
    // still the truthful answer — a person owes an answer — and the alternative
    // readings are both wrong: `Pending` would claim this call found a card it
    // cannot see, and anything terminal would end a delivery over a lost race.
    assert_eq!(found(None, false), Requested::Raised);
}

#[test]
fn a_pending_grant_whose_card_was_re_raised_is_raised_not_suppressed() {
    // The sweeper expires a card at 30 days and leaves the grant pending. The
    // next request writes a fresh card, and reporting `Pending` there would
    // claim a question is open when the only open one is the grant.
    assert_eq!(found(Some(status::PENDING), true), Requested::Raised);
}

#[test]
fn a_pending_grant_with_its_card_still_open_suppresses() {
    // The redelivery cadence is one second. This is the arm that makes it one
    // question rather than sixty a minute.
    assert_eq!(found(Some(status::PENDING), false), Requested::Pending);
}

#[test]
fn a_standing_yes_is_nothing_to_ask_about() {
    assert_eq!(found(Some(status::APPROVED), false), Requested::Approved);
}

#[test]
fn only_a_revoked_grant_is_read_as_a_persons_no() {
    // The property, not the branch: `Denied` is the one answer that ends an
    // event, so it must be reachable from exactly one status. Any other
    // spelling that produced it would end deliveries nobody refused.
    let vocabulary = [
        None,
        Some(status::PENDING),
        Some(status::APPROVED),
        Some(status::REVOKED),
        Some("quarantined"),
        Some("PENDING"),
        Some(""),
    ];
    for spelling in vocabulary {
        let ends = found(spelling, false) == Requested::Denied;
        assert_eq!(
            ends,
            spelling == Some(status::REVOKED),
            "{spelling:?} must{} end the event",
            if spelling == Some(status::REVOKED) {
                ""
            } else {
                " not"
            }
        );
    }
}

#[test]
fn a_status_this_build_cannot_place_waits() {
    // The fail-safe direction. An unknown spelling is a build that is behind
    // its own datastore, and waiting leaves the question answerable; ending
    // would throw the delivery away on a word we simply do not know yet.
    assert_eq!(found(Some("quarantined"), false), Requested::Pending);
    assert_eq!(found(Some("PENDING"), false), Requested::Pending);
}

#[test]
fn evidence_carries_the_key_the_approve_statement_joins_on() {
    // `RESOLVE_GATE` joins `g.service = r.evidence->>'service'`. A card written
    // without that key resolves cleanly, moves no grant, and leaves the fleet
    // where it was — the failure with no error this module exists to end. The
    // service, never the fleet's own name for the credential, is what it holds.
    let evidence = Wanted {
        service: "github",
        credential: "gh",
        origin: Origin::Install,
    }
    .evidence();
    let parsed: serde_json::Value =
        serde_json::from_str(&evidence).expect("the evidence must be an object");
    assert_eq!(
        parsed.get("service").and_then(serde_json::Value::as_str),
        Some("github")
    );
}

#[test]
fn the_two_origins_stay_distinguishable_on_the_row_and_in_the_metric() {
    // One fact read two ways. If either pair collapsed, a card would say it came
    // from the install while the metric counted it as a park.
    assert_ne!(Origin::Install.reason(), Origin::Park.reason());
    assert_ne!(Origin::Install.as_str(), Origin::Park.as_str());
}
