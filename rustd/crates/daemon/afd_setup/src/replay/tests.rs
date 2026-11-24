//! What a read receipt answers, without a database.
#![expect(clippy::expect_used, reason = "a wrong answer fails the proof loudly")]

use super::{Receipt, bound, decide};
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
