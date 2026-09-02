//! The two knobs a rotation moves between, read as a pair.
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; an unmet precondition should stop it"
)]
use super::{QSTASH_CURRENT_KEY_KNOB, QSTASH_NEXT_KEY_KNOB, optional, signing_keys};
use afd_core::env::MapEnv;

/// A key this fixture configures; the value is never parsed, only carried.
const CURRENT: &str = "sig_current_fixture";

/// The key a rotation moves to.
const NEXT: &str = "sig_next_fixture";

/// An exported-but-blank knob is unset, not a value.
///
/// The distinction is not cosmetic: a blank that read as configured would
/// be sent upstream as a bare `Bearer `, which the vendor refuses with a
/// sentence naming nothing an operator can act on. Whitespace is trimmed
/// for the same reason — a knob set from a shell heredoc arrives with a
/// newline attached.
#[test]
fn a_blank_or_whitespace_knob_is_absent_rather_than_empty() {
    let source = MapEnv::from_pairs([
        ("SET", "value"),
        ("BLANK", ""),
        ("SPACES", "   "),
        ("PADDED", "  value  "),
    ]);

    assert_eq!(optional(&source, "SET").as_deref(), Some("value"));
    assert_eq!(
        optional(&source, "BLANK"),
        None,
        "an exported empty string meant unset"
    );
    assert_eq!(
        optional(&source, "SPACES"),
        None,
        "whitespace is not a value"
    );
    assert_eq!(
        optional(&source, "PADDED").as_deref(),
        Some("value"),
        "a knob set from a heredoc arrives padded and is still that value"
    );
    assert_eq!(optional(&source, "ABSENT"), None);
}

/// Both keys or neither — a half-rotation is no configuration.
///
/// The failure this prevents is delayed and total. A verifier holding only
/// the current key works right up until the vendor rotates to the next one,
/// and then refuses EVERY delivery — an outage that begins on the vendor's
/// schedule rather than on any deploy of ours. Treating half as none makes
/// it a loud refusal at the first fire instead.
#[test]
fn one_signing_key_is_no_configuration_rather_than_half_of_one() {
    let neither = MapEnv::from_pairs([]);
    assert!(
        signing_keys(&neither).is_none(),
        "neither key is unconfigured"
    );

    let current_only = MapEnv::from_pairs([(QSTASH_CURRENT_KEY_KNOB, CURRENT)]);
    assert!(
        signing_keys(&current_only).is_none(),
        "the current key alone is a rotation half-done — it verifies until \
         the vendor rotates and then refuses everything"
    );

    let next_only = MapEnv::from_pairs([(QSTASH_NEXT_KEY_KNOB, NEXT)]);
    assert!(
        signing_keys(&next_only).is_none(),
        "the next key alone is the same half"
    );

    let both = MapEnv::from_pairs([
        (QSTASH_CURRENT_KEY_KNOB, CURRENT),
        (QSTASH_NEXT_KEY_KNOB, NEXT),
    ]);
    let keys = signing_keys(&both).expect("both keys configured is a configuration");
    assert_eq!(keys.current, CURRENT);
    assert_eq!(keys.next, NEXT);
}
