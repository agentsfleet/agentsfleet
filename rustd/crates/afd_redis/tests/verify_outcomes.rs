//! Every tag the shared Lua script can return, read as the outcome it means.
//!
//! The script's return contract is nine tags. A live test only exercises the
//! ones its scenario produces, so the mapping is checked here in full: a tag
//! read as the wrong outcome is a device-flow bug — `rate_limited` mistaken for
//! `invalid_code` tells a user to try again on a session that is already dead,
//! and `replay` mistaken for `success` emits a second audit event for one
//! redemption.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_redis::session::{VerifyOutcome, VerifyPayload, outcome_from_reply};

fn reply(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|part| (*part).to_owned()).collect()
}

/// The two payload-carrying tags, which differ only in what they mean.
#[test]
fn test_payload_tags_carry_their_three_fields() {
    let payload = VerifyPayload {
        dashboard_public_key: "dpk".to_owned(),
        ciphertext: "ct".to_owned(),
        nonce: "n".to_owned(),
    };

    assert_eq!(
        outcome_from_reply(&reply(&["success", "dpk", "ct", "n"])).expect("a success reply"),
        VerifyOutcome::Success(payload.clone())
    );
    assert_eq!(
        outcome_from_reply(&reply(&["replay", "dpk", "ct", "n"])).expect("a replay reply"),
        VerifyOutcome::Replay(payload),
        "a replay is the same payload and a different event — never a second success"
    );
}

/// The tags that carry nothing.
#[test]
fn test_bare_tags_map_one_to_one() {
    for (tag, expected) in [
        ("missing", VerifyOutcome::Missing),
        ("expired", VerifyOutcome::Expired),
        ("consumed", VerifyOutcome::Consumed),
        ("not_approved", VerifyOutcome::NotApproved),
        ("rate_limited", VerifyOutcome::RateLimited),
    ] {
        assert_eq!(
            outcome_from_reply(&reply(&[tag])).expect("a bare tag"),
            expected,
            "{tag} was read as something else"
        );
    }
}

/// The two tags that carry one field.
#[test]
fn test_tags_with_one_field() {
    assert_eq!(
        outcome_from_reply(&reply(&["aborted", "rate_limit_exceeded"])).expect("an abort reply"),
        VerifyOutcome::Aborted("rate_limit_exceeded".to_owned()),
        "the reason is what an operator reads; dropping it loses the why"
    );
    assert_eq!(
        outcome_from_reply(&reply(&["invalid_code", "3"])).expect("an invalid-code reply"),
        VerifyOutcome::InvalidCode(3),
        "the attempt count is what the caller shows the user"
    );
}

/// A reply this client does not understand is refused, not guessed at.
#[test]
fn test_unknown_and_malformed_replies_are_refused() {
    for bad in [
        vec![],
        reply(&["something_new"]),
        reply(&["invalid_code"]),
        reply(&["invalid_code", "not-a-number"]),
        reply(&["success", "dpk"]),
    ] {
        let error = outcome_from_reply(&bad).expect_err("must be refused");
        assert!(error.is_command(), "{bad:?} gave {error}");
    }
}

/// An attempt count past the byte the session tracks it in is refused rather
/// than wrapping to a small number that reads as "plenty of tries left".
#[test]
fn test_an_impossible_attempt_count_is_refused() {
    let error = outcome_from_reply(&reply(&["invalid_code", "300"])).expect_err("must be refused");
    assert!(error.is_command(), "got {error}");
}
