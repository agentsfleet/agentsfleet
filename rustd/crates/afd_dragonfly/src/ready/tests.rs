//! The readiness token, which decides something without a datastore: whether
//! two marks can ever be mistaken for one generation.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the restriction set is for the daemon"
)]

use afd_core::clock::UnixMillis;
use afd_crypto::entropy::Entropy;

use super::ReadyToken;

/// An instant every test here mints at, so the two tokens a test compares
/// can differ only by their random half.
const SAME_INSTANT: UnixMillis = UnixMillis::from_millis(1_759_000_000_000);

/// Two marks of one fleet in the same millisecond still mint two tokens, so a
/// poll holding the first cannot clear the second.
#[test]
fn test_marks_mint_distinct_tokens() {
    let entropy = Entropy::new();
    let first = ReadyToken::mint(&entropy, SAME_INSTANT).expect("the host has entropy");
    let second = ReadyToken::mint(&entropy, SAME_INSTANT).expect("the host has entropy");

    assert_ne!(
        first, second,
        "two marks at one instant wrote one generation, so a stale clear would match a newer mark"
    );
}

/// A token is a canonical version-7 identifier, which is what the index
/// stores and what `clear_if_unchanged` compares byte for byte.
#[test]
fn a_minted_token_is_a_canonical_version_7_identifier() {
    let token = ReadyToken::mint(&Entropy::new(), SAME_INSTANT).expect("the host has entropy");

    let parsed = afd_core::id::Uuid7::parse(token.as_str()).expect("a token parses as a UUIDv7");
    assert_eq!(
        parsed.as_str(),
        token.as_str(),
        "the stored spelling is canonical"
    );
}

/// The random half is the source's, not the process's: a pinned draw mints
/// the identifier those bytes encode.
#[cfg(feature = "test-util")]
#[test]
fn a_token_is_drawn_from_the_index_entropy_source() {
    let (entropy, ctrl) = Entropy::new_mocked();
    let bytes = [7_u8; afd_core::id::ENTROPY_LEN];
    ctrl.push_bytes(&bytes);

    let token = ReadyToken::mint(&entropy, SAME_INSTANT).expect("a queued draw mints");

    let expected = afd_core::id::Uuid7::encode(SAME_INSTANT, bytes).expect("a valid instant");
    assert_eq!(token.as_str(), expected.as_str());
}

/// A refused draw is an unmintable error, not a mark written under a
/// predictable value.
#[cfg(feature = "test-util")]
#[test]
fn a_refused_draw_mints_nothing() {
    let (entropy, ctrl) = Entropy::new_mocked();
    ctrl.fail_next();

    let refused = ReadyToken::mint(&entropy, SAME_INSTANT).expect_err("the draw was refused");

    assert!(refused.is_unmintable(), "{refused}");
    assert!(!refused.is_unavailable(), "the datastore did nothing wrong");
}

/// An instant with no version-7 spelling is refused the same way.
#[test]
fn an_instant_before_the_epoch_mints_nothing() {
    let refused = ReadyToken::mint(&Entropy::new(), UnixMillis::from_millis(-1))
        .expect_err("a negative instant has no version-7 spelling");

    assert!(refused.is_unmintable(), "{refused}");
}
