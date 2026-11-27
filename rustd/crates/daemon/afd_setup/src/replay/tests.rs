//! What a read receipt answers, without a database.
#![expect(clippy::expect_used, reason = "a wrong answer fails the proof loudly")]

use super::{Receipt, bound, decide, digest};
use af_core::error_code::{self, ErrorCode};
use af_core::id::Uuid7;

const ACTOR: &str = "019f0000-0000-7000-8000-000000000001";
const OTHER: &str = "019f0000-0000-7000-8000-000000000002";
const RESOURCE: &str = "019f0000-0000-7000-8000-000000000003";
const DIGEST: &str = "setup:v1:sha256:aa";

/// The receipt `user` filed for a request whose digest is `digest`.
fn receipt(user: &str, digest: &str, live: bool) -> Receipt {
    let user = Uuid7::parse(user).expect("a canonical actor");
    Receipt {
        request_digest: bound(&user, digest).expect("a bound digest"),
        resource_id: Uuid7::parse(RESOURCE).expect("a canonical resource"),
        live,
    }
}

#[test]
fn a_receipt_answers_by_actor_body_and_resource() {
    let actor = Uuid7::parse(ACTOR).expect("actor");
    let refused: Option<ErrorCode> = None;
    let cases = [
        ("no receipt is a new key", None, refused),
        (
            "same actor and body replay the resource",
            Some(receipt(ACTOR, DIGEST, true)),
            None,
        ),
        (
            "another actor is refused",
            Some(receipt(OTHER, DIGEST, true)),
            Some(error_code::SETUP_REPLAY),
        ),
        (
            "a changed body is refused",
            Some(receipt(ACTOR, "setup:v1:sha256:bb", true)),
            Some(error_code::SETUP_REPLAY),
        ),
        (
            "a deleted resource is gone",
            Some(receipt(ACTOR, DIGEST, false)),
            Some(error_code::SETUP_GONE),
        ),
        (
            "a changed body is refused before a deletion is reported",
            Some(receipt(ACTOR, "setup:v1:sha256:bb", false)),
            Some(error_code::SETUP_REPLAY),
        ),
    ];
    for (case, read, expected) in cases {
        let replayed = read.as_ref().is_some_and(|row| row.live);
        let answer = decide(read, &actor, DIGEST);
        match expected {
            Some(code) => assert_eq!(answer.expect_err(case).code(), code, "{case}"),
            None => assert_eq!(
                answer.expect(case).map(|id| id.as_str().to_owned()),
                replayed.then(|| RESOURCE.to_owned()),
                "{case}"
            ),
        }
    }
}

/// A receipt row stores this digest, so it is pinned to the byte: the sorted
/// JSON, the plain SHA-256 over it, lowercase hex and the versioned prefix.
#[test]
fn the_stored_digest_is_pinned_to_the_byte() {
    let actor = Uuid7::parse(ACTOR).expect("actor");
    let body = serde_json::json!({ "name": "morning-rooster", "a": [1, { "z": true, "b": null }] });
    // pin test: literal is the contract
    assert_eq!(
        digest(&body).expect("a digest"),
        "setup:v1:sha256:66908c9795201959791de0b5865e835f631ce875a545ead249f52d54c612bb51"
    );
    // pin test: literal is the contract
    assert_eq!(
        bound(&actor, DIGEST).expect("a bound digest"),
        "setup:v1:sha256:145df32c2e41089eda20784a7da4016e81f6927c533fc70de33f0de160c3155d"
    );
}
