//! A chosen name's bound, the blank that means "generate", and the generator.
//!
//! Lifted out of `name.rs` at the file cap, the first cut the length rule
//! asks for.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]
use afd_crypto::entropy::Entropy;

use super::{Chosen, MAX_NAME_CODEPOINTS, SEPARATOR, SUFFIX_ALPHABET, SUFFIX_LEN, generate};
use crate::error;

#[test]
fn test_blank_workspace_name_still_generates_one() {
    // The bound is garde's now, and it is declared on the trimmed value, so a
    // blank name never reaches it: blank still means "generate one".
    assert_eq!(Chosen::parse("  ").ok(), Some(None));
    let refused = Chosen::parse(&"名".repeat(MAX_NAME_CODEPOINTS + 1))
        .err()
        .map(|refusal| (refusal.code(), refusal.to_string()));
    let too_long = error::workspace_name_too_long();
    assert_eq!(
        refused,
        Some((too_long.code(), too_long.to_string())),
        "129 code points answer the too-long refusal"
    );
}

#[test]
fn a_chosen_name_is_trimmed_and_kept() {
    let chosen = Chosen::parse("  deploy bots\t").expect("a plain name passes");
    assert_eq!(
        chosen.expect("a non-blank name is a choice").as_str(),
        "deploy bots"
    );
}

#[test]
fn choosing_nothing_in_any_spelling_means_generate() {
    // Empty, ASCII whitespace, and whitespace only Unicode can spell —
    // each is "no choice", never a 400.
    for blank in ["", "   ", "\t\r\n", "\u{00a0}\u{3000}"] {
        let outcome = Chosen::parse(blank).expect("blankness is not an error");
        assert!(outcome.is_none(), "{blank:?} is not a name anyone chose");
    }
}

#[test]
fn the_cap_counts_code_points_at_the_boundary() {
    let at_cap = "é".repeat(MAX_NAME_CODEPOINTS);
    assert!(
        Chosen::parse(&at_cap)
            .expect("the cap itself passes")
            .is_some(),
        "128 code points is within the rule"
    );
    let past_cap = "é".repeat(MAX_NAME_CODEPOINTS + 1);
    assert!(
        Chosen::parse(&past_cap).is_err(),
        "129 code points is past it, whatever the byte count"
    );
}

#[test]
fn a_character_that_lets_a_name_lie_is_refused() {
    // One representative per forbidden class: C0, C1, the Arabic letter
    // mark, a directional mark, a line separator, an override, an isolate.
    for lying in [
        "tab\u{0007}",
        "c1\u{0085}",
        "alm\u{061c}",
        "mark\u{200e}",
        "sep\u{2028}",
        "bidi\u{202e}",
        "iso\u{2066}",
    ] {
        assert!(
            Chosen::parse(lying).is_err(),
            "{lying:?} carries a character no stored name may"
        );
    }
}

#[test]
fn a_generated_name_has_the_documented_shape() {
    let name = generate(&Entropy::new()).expect("a host can draw random bytes");
    let parts: Vec<&str> = name.split(SEPARATOR).collect();

    assert_eq!(
        parts.len(),
        3,
        "the shape is adjective-noun-suffix, got {name}"
    );
    assert!(
        parts.iter().all(|part| !part.is_empty()),
        "no part may be empty, got {name}"
    );
    let suffix = parts.last().expect("a three-part name has a last part");
    assert_eq!(
        suffix.len(),
        SUFFIX_LEN,
        "the suffix is a fixed width so names line up in a list, got {name}"
    );
}

#[test]
fn a_name_survives_a_url_and_a_terminal_unquoted() {
    // The whole reason for a generated name is that a person reads it back
    // and types it somewhere. Every character has to be one that survives
    // that trip without escaping.
    for _draw in 0..64 {
        let name = generate(&Entropy::new()).expect("a host can draw random bytes");
        assert!(
            name.bytes().all(|byte| byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || byte == SEPARATOR as u8),
            "{name} carries a character that would need quoting"
        );
    }
}

#[test]
fn the_suffix_avoids_the_characters_people_misread() {
    // `l`/`1` and `o`/`0` are the pairs somebody transcribing a name off a
    // support ticket gets wrong. This asserts the alphabet, not a sample,
    // because a sample would pass by luck.
    for ambiguous in *b"loi01" {
        assert!(
            !SUFFIX_ALPHABET.contains(&ambiguous),
            "{} is a character people mistype",
            char::from(ambiguous)
        );
    }
}

#[test]
fn two_names_in_a_row_differ() {
    // Not a distribution proof — that belongs to the entropy source, which
    // has its own. This catches the specific regression of a generator that
    // draws once and reuses, which would make the unique index the only
    // thing standing between a tenant and one workspace.
    let first = generate(&Entropy::new()).expect("a host can draw random bytes");
    let second = generate(&Entropy::new()).expect("a host can draw random bytes");
    assert_ne!(
        first, second,
        "a generator that repeats turns every create after the first into a retry"
    );
}
