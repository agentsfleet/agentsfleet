//! The correlation token a refusal carries.
//!
//! The format is the assertion worth having. `req_` plus twelve lowercase hex
//! characters is what `handlers/common.zig` writes, what the dashboard shows,
//! and what somebody types into a support ticket from a screenshot — so a
//! change to it is a change to a human process, not just to a string.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::collections::HashSet;

use afd_api::{RequestId, UNKNOWN_REQUEST_ID};
use afd_crypto::entropy::Entropy;

/// Characters after the `req_` prefix.
const HEX_LEN: usize = 12;

/// A minted id has the shape every consumer of it expects.
#[test]
fn test_a_minted_id_is_the_zig_format() {
    let id = RequestId::mint();
    let text = id.as_str();

    let hex = text
        .strip_prefix("req_")
        .expect("every request id is prefixed `req_`");
    assert_eq!(hex.len(), HEX_LEN, "id {text} is not twelve hex characters");
    assert!(
        hex.bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "id {text} carries something other than lowercase hex"
    );
}

/// The bytes reach the text exactly, including the ones that render as zero.
///
/// The id is packed into the low forty-eight bits of an integer and rendered
/// once, so a value with leading zero bytes is precisely where a missing width
/// specifier would show up — and `req_1` would still pass a "starts with req_"
/// assertion while being four characters short of an id anyone can look up.
#[test]
fn test_pinned_bytes_render_to_a_pinned_id() {
    let (entropy, ctrl) = Entropy::new_mocked();

    ctrl.push_bytes(&[0x8f, 0x21, 0xc4, 0x0b, 0xa9, 0xde]);
    assert_eq!(RequestId::mint_from(&entropy).as_str(), "req_8f21c40ba9de");

    ctrl.push_bytes(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x01]);
    assert_eq!(
        RequestId::mint_from(&entropy).as_str(),
        "req_000000000001",
        "the leading zero bytes are part of the id, not padding to drop"
    );

    ctrl.push_bytes(&[0xff, 0xff, 0xff, 0xff, 0xff, 0xff]);
    assert_eq!(RequestId::mint_from(&entropy).as_str(), "req_ffffffffffff");
}

/// Two ids minted in a row are two different ids.
///
/// Weak as a randomness claim and not meant as one: what it catches is a minter
/// that has stopped drawing — a cached value, a zeroed buffer — which is the
/// failure that would make every record in a storm carry the same token.
#[test]
fn test_ids_do_not_repeat() {
    let minted: HashSet<String> = (0..64).map(|_| RequestId::mint().into()).collect();
    assert_eq!(minted.len(), 64, "a minted id repeated within 64 draws");
}

/// A host that cannot produce entropy still answers, and says so.
///
/// The sentinel is reached on one condition and means one thing. That is the
/// property, and it is the reason the id is minted here rather than read out of
/// a request extension some layer may or may not have stamped: a sentinel that
/// also meant "nobody stamped one" would be unreadable as a signal.
#[test]
fn test_an_entropy_failure_answers_the_sentinel() {
    let (entropy, ctrl) = Entropy::new_mocked();
    ctrl.fail_next();

    let id = RequestId::mint_from(&entropy);
    assert_eq!(id.as_str(), UNKNOWN_REQUEST_ID);
    assert_eq!(UNKNOWN_REQUEST_ID, "req_unknown", "common.zig's constant");
}

/// The three ways to read an id agree.
#[test]
fn test_every_reading_of_an_id_is_the_same_text() {
    let (entropy, ctrl) = Entropy::new_mocked();
    ctrl.push_bytes(&[0x01, 0x23, 0x45, 0x67, 0x89, 0xab]);
    let id = RequestId::mint_from(&entropy);

    let borrowed = id.as_str().to_owned();
    let displayed = id.to_string();
    let owned: String = id.into();

    assert_eq!(borrowed, "req_0123456789ab");
    assert_eq!(displayed, borrowed);
    assert_eq!(owned, borrowed);
}
