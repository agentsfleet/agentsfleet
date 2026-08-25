//! Dimension 1.2 — the associated data is byte-identical to the daemon's.
//!
//! This is the one place envelope parity actually breaks. The primitive is a
//! standard covered by published vectors; the layout is six fixed-width
//! components. What is bespoke — and therefore what drifts — is the string the
//! two implementations agree to authenticate. A single tidy-up here makes every
//! row the Zig daemon ever wrote fail to open, with no compile error and no
//! type change to notice.
//!
//! Transcribed from `crypto_store_write.zig`:
//!
//! ```text
//! const AAD_SEPARATOR: u8 = 0x1f;
//! const AAD_FORMAT = "{s}{c}{s}{c}{d}";
//! buildAad: allocLowerString(workspace_id), SEP, key_name, SEP, kek_version
//! ```
#![expect(
    clippy::unwrap_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_crypto::aad::Aad;

/// The exact bytes, spelled out rather than rebuilt with the same code twice.
#[test]
fn test_aad_matches_zig_format() {
    let aad = Aad::new("ws_0123", "openai");
    assert_eq!(aad.as_bytes(), b"ws_0123\x1fopenai\x1f2");
}

/// The asymmetry, which is the part a cleanup would "fix" and thereby break.
///
/// `std.ascii.allocLowerString` is applied to the workspace identifier ALONE on
/// the Zig side. Lowercasing the key name too would look tidier and would orphan
/// every credential stored under a name containing a capital letter.
#[test]
fn test_aad_lowercases_workspace_but_not_key_name() {
    let aad = Aad::new("WS_ABC", "MixedCaseName");
    assert_eq!(aad.as_bytes(), b"ws_abc\x1fMixedCaseName\x1f2");

    let text = String::from_utf8(aad.as_bytes().to_vec()).unwrap();
    assert!(
        text.contains("MixedCaseName"),
        "key name must survive verbatim"
    );
    assert!(!text.contains("WS_ABC"), "workspace id must be lowercased");
}

/// The separator is the ASCII unit separator, not a comma, colon or NUL.
///
/// Asserted by value because every plausible alternative is also invisible in a
/// terminal, so a wrong one reads as correct right up until decryption fails.
#[test]
fn test_aad_separator_is_the_unit_separator() {
    let aad = Aad::new("a", "b");
    // The exact byte sequence, which pins the separator's value AND both of its
    // positions in one assertion — a count would say less and needs a dependency.
    assert_eq!(aad.as_bytes(), &[b'a', 0x1f, b'b', 0x1f, b'2']);
}

/// The version is rendered as decimal text, not as a raw byte.
#[test]
fn test_aad_version_is_decimal_text() {
    assert_eq!(Aad::versioned("w", "k", 2).as_bytes(), b"w\x1fk\x1f2");
    assert_eq!(Aad::versioned("w", "k", 10).as_bytes(), b"w\x1fk\x1f10");
    // A raw byte would be 0x02 and one byte long; text is '2' and also one byte
    // long, which is exactly why this needs asserting rather than eyeballing.
    assert_eq!(Aad::versioned("w", "k", 2).as_bytes().last(), Some(&b'2'));
}

/// Multi-byte names pass through as UTF-8, not as escaped or truncated ASCII.
#[test]
fn test_aad_carries_utf8_key_names() {
    let aad = Aad::new("ws_1", "café-ключ");
    assert_eq!(aad.as_bytes(), "ws_1\u{1f}café-ключ\u{1f}2".as_bytes());
}

/// Empty fields still produce both separators, so the shape never collapses.
#[test]
fn test_aad_keeps_both_separators_when_fields_are_empty() {
    assert_eq!(Aad::new("", "").as_bytes(), b"\x1f\x1f2");
}

/// The rendering names the fields without inventing a secret to redact.
#[test]
fn test_aad_debug_shows_the_bytes() {
    let rendered = format!("{:?}", Aad::new("ws_1", "openai"));
    assert!(rendered.starts_with("Aad("), "got {rendered}");
    assert!(rendered.contains("openai"), "got {rendered}");
}
