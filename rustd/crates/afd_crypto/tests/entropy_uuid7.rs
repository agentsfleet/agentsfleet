//! `Entropy::uuid7` is the one way a row identifier is minted, so its three
//! outcomes are pinned here: the instant and the drawn bytes both reach the
//! identifier, a refused draw reports as entropy, and an instant the identifier
//! cannot carry keeps the identifier layer's refusal as its cause.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::error::Error as _;

use afd_core::clock::UnixMillis;
use afd_core::error_code;
use afd_core::id::{ENTROPY_LEN, Uuid7};
use afd_crypto::entropy::Entropy;

/// An instant well inside the range a version-7 identifier carries.
const AT: UnixMillis = UnixMillis::from_millis(1_767_225_600_000);

/// The bytes the mocked source hands the next draw.
const DRAWN: [u8; ENTROPY_LEN] = [0x5A; ENTROPY_LEN];

/// The identifier is exactly what `Uuid7::encode` makes of the same instant
/// and the same bytes, so neither half is dropped or re-drawn on the way.
#[test]
fn test_uuid7_stamps_the_instant_and_uses_the_drawn_bytes() {
    let (entropy, control) = Entropy::new_mocked();
    control.push_bytes(&DRAWN);

    let minted = entropy.uuid7(AT).expect("a healthy source mints");

    assert_eq!(
        minted,
        Uuid7::encode(AT, DRAWN).expect("the fixture instant encodes")
    );
}

/// A source that refuses the draw answers as entropy, the kind every caller
/// already maps to its internal failure.
#[test]
fn test_uuid7_reports_a_refused_draw_as_entropy() {
    let (entropy, control) = Entropy::new_mocked();
    control.fail_next();

    let refused = entropy.uuid7(AT).expect_err("a refused draw mints nothing");

    assert!(refused.is_entropy(), "got {refused}");
}

/// An instant before the Unix epoch has no version-7 spelling. The refusal is
/// this instance's problem, and the identifier layer's reason stays reachable
/// through `source()` rather than being restated.
#[test]
fn test_uuid7_keeps_the_identifier_refusal_as_its_cause() {
    let (entropy, control) = Entropy::new_mocked();
    control.push_bytes(&DRAWN);

    let refused = entropy
        .uuid7(UnixMillis::from_millis(-1))
        .expect_err("a pre-epoch instant has no identifier");

    assert!(!refused.is_entropy(), "the draw succeeded: {refused}");
    assert_eq!(refused.code(), error_code::INTERNAL_OPERATION_FAILED);
    let cause = refused
        .source()
        .and_then(|cause| cause.downcast_ref::<afd_core::error::Error>())
        .expect("the identifier layer's own error is kept as the cause");
    let direct = Uuid7::encode(UnixMillis::from_millis(-1), DRAWN)
        .expect_err("the same instant refuses directly");
    assert!(cause.is_id_shape(), "got {cause}");
    assert_eq!(cause.code(), error_code::UUIDV7_INVALID_ID_SHAPE);
    assert_eq!(cause.code(), direct.code());
}
