//! Identity validation: the accept/reject set must match `id_format.zig` exactly.
#![expect(
    clippy::unwrap_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::id::Uuid7;

/// A canonical version-7 identifier: lowercase hex, dashes at 8/13/18/23,
/// version nibble `7` at offset 14, RFC 4122 variant `8` at offset 19.
const CANONICAL: &str = "0198f3a2-7c1b-7def-8abc-1234567890ab";

#[test]
fn should_accept_a_canonical_lowercase_version_7_identifier() {
    let id = Uuid7::parse(CANONICAL).unwrap();
    assert_eq!(id.as_str(), CANONICAL);
    assert_eq!(id.to_string(), CANONICAL);
}

/// Catches the bug the Zig module exists to prevent: one entity reachable under
/// two spellings, because Postgres folds `::uuid` to lowercase while every
/// text-keyed store (Redis dedupe keys, session keys, string equality) does not.
#[test]
fn test_uuid_v7_rejects_uppercase() {
    let upper = CANONICAL.to_uppercase();
    // Precondition: the value differs only in case, so nothing else can explain
    // a rejection.
    assert_eq!(upper.to_lowercase(), CANONICAL);

    let err = Uuid7::parse(&upper).unwrap_err();
    assert!(err.is_id_shape(), "expected a shape failure, got {err}");
    assert_eq!(err.code().as_str(), "UZ-UUIDV7-009");
    assert!(
        err.to_string().contains("lowercase hex"),
        "error should name the rule it broke: {err}"
    );
}

/// Rejection, never normalization: a caller must not be able to recover a
/// lowercase value from the uppercase input by round-tripping it.
#[test]
fn should_not_normalize_uppercase_into_a_valid_identifier() {
    for upper in [
        CANONICAL.to_uppercase().as_str(),
        "0198F3A2-7C1B-7DEF-8ABC-1234567890AB",
        // A single uppercase character is enough.
        "0198f3a2-7c1b-7def-8abc-1234567890aB",
    ] {
        let err = Uuid7::parse(upper).unwrap_err();
        assert!(err.is_id_shape(), "{upper}: {err}");
    }
}

#[test]
fn should_reject_lengths_other_than_36() {
    for wrong in ["", "0198f3a2", &CANONICAL[..35], &format!("{CANONICAL}a")] {
        let err = Uuid7::parse(wrong).unwrap_err();
        assert!(err.to_string().contains("36 characters"), "input {wrong:?}");
    }
}

#[test]
fn should_reject_a_dash_out_of_position() {
    // Dash moved from offset 8 to offset 7; length still 36.
    let moved = "0198f3a-27c1b-7def-8abc-1234567890ab";
    assert_eq!(moved.len(), 36);
    let err = Uuid7::parse(moved).unwrap_err();
    assert!(
        err.to_string().contains("dashes at 8, 13, 18 and 23"),
        "{err}"
    );
}

#[test]
fn should_reject_non_hex_characters() {
    // 'g' is outside a-f.
    let err = Uuid7::parse("0198f3a2-7c1b-7def-8abc-1234567890ag").unwrap_err();
    assert!(err.is_id_shape(), "{err}");
    // Unicode that happens to be 36 BYTES must not slip through a length check
    // that counts bytes; it is 22 characters.
    let cjk = "日本語語-7c1b-7def-8abc-12345678";
    assert_eq!(cjk.len(), 36, "precondition: 36 bytes");
    assert_eq!(
        cjk.chars().count(),
        28,
        "precondition: fewer than 36 characters"
    );
    assert!(Uuid7::parse(cjk).unwrap_err().is_id_shape());
}

#[test]
fn should_reject_a_version_nibble_other_than_7() {
    // Version 4 in an otherwise canonical identifier.
    let v4 = "0198f3a2-7c1b-4def-8abc-1234567890ab";
    let err = Uuid7::parse(v4).unwrap_err();
    assert!(err.to_string().contains("version nibble"), "{err}");
}

#[test]
fn should_reject_a_variant_outside_rfc_4122() {
    // Variant nibble must be 8, 9, a or b.
    for variant in ['0', '7', 'c', 'f'] {
        let mut text = String::from(CANONICAL);
        text.replace_range(19..20, &variant.to_string());
        let err = Uuid7::parse(&text).unwrap_err();
        assert!(
            err.to_string().contains("variant nibble"),
            "{variant}: {err}"
        );
    }
    for variant in ['8', '9', 'a', 'b'] {
        let mut text = String::from(CANONICAL);
        text.replace_range(19..20, &variant.to_string());
        Uuid7::parse(&text).unwrap_or_else(|e| unreachable!("variant {variant} is RFC 4122: {e}"));
    }
}

#[test]
fn should_round_trip_through_serde_unchanged() {
    let id = Uuid7::parse(CANONICAL).unwrap();
    let json = serde_json::to_string(&id).unwrap();
    assert_eq!(json, format!("\"{CANONICAL}\""));
    assert_eq!(serde_json::from_str::<Uuid7>(&json).unwrap(), id);
}

/// A JSON producer may escape any character. The Zig parser unescapes before it
/// validates, so an escaped-but-canonical identifier is accepted there; a
/// borrowing deserializer here would reject it and call the divergence an
/// optimization.
#[test]
fn should_accept_an_escaped_but_canonical_identifier() {
    // `-` is '-'; the unescaped text is exactly CANONICAL.
    let escaped = "\"0198f3a2\\u002d7c1b\\u002d7def\\u002d8abc\\u002d1234567890ab\"";
    let id: Uuid7 = serde_json::from_str(escaped).unwrap();
    assert_eq!(id.as_str(), CANONICAL);
}

#[test]
fn should_reject_uppercase_through_serde_too() {
    let json = format!("\"{}\"", CANONICAL.to_uppercase());
    let err = serde_json::from_str::<Uuid7>(&json).unwrap_err();
    assert!(err.to_string().contains("lowercase hex"), "{err}");
}
