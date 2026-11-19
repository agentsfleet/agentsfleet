//! Hashline's tags on text alone: the hash, the lines it is taken over, and
//! the radius a tag is looked for in.

#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use super::{Line, Located, Target, lines, tag, tagged};
use crate::tools::code;

/// The tag is Fowler–Noll–Vo 1a over the trimmed line before, a bar, and the
/// trimmed line, keeping twelve bits as three hex digits.
#[test]
fn a_tag_is_the_low_twelve_bits_of_fnv1a_over_parent_bar_line() {
    // pin test: literal is the contract
    assert_eq!(tag("parent", "child"), "6eb");
    // pin test: literal is the contract
    assert_eq!(tag("one", "two"), "0cd");
    // pin test: literal is the contract
    assert_eq!(tag("", ""), "a3b");
    assert_ne!(
        tag("a", "same"),
        tag("b", "same"),
        "the line before is in the tag"
    );
    assert_eq!(
        tag("  a\t", "same \r\n"),
        tag("a", "same"),
        "edges are trimmed"
    );
}

#[test]
fn lines_keep_their_starts_and_a_trailing_empty_segment() {
    let text = "ab\n\ncd\n";

    let collected: Vec<(usize, &str)> = lines(text)
        .iter()
        .map(|line| (line.start, line.text))
        .collect();

    assert_eq!(collected, [(0, "ab"), (3, ""), (4, "cd"), (7, "")]);
    assert_eq!(
        tagged("").lines().count(),
        1,
        "an empty file is one empty line"
    );
}

#[test]
fn a_tag_is_looked_for_within_the_radius_only() {
    let text = (0..200)
        .map(|number| format!("line {number}\n"))
        .collect::<Vec<String>>()
        .concat();
    let lines: Vec<Line<'_>> = lines(&text);
    let wanted = Target {
        line: 150,
        hash: &tag("line 148", "line 149"),
    };

    assert_eq!(wanted.found_near(&lines, 149), Located::At(149));
    assert_eq!(
        wanted.found_near(&lines, 100),
        Located::At(149),
        "fifty away is found"
    );
    assert_eq!(
        wanted.found_near(&lines, 98),
        Located::Missing,
        "fifty-one away is not"
    );
}

#[test]
fn a_tag_that_does_not_parse_or_names_line_zero_is_invalid() {
    for given in ["10:abc", "L10abc", "L10:ab", "L0:abc", "Lten:abc", ""] {
        let refused = Target::parse(given).unwrap_err();

        assert_eq!(
            refused.error_code,
            Some(code::INVALID_ARGUMENTS),
            "{given:?}"
        );
        assert!(refused.text.contains("is not a tag"), "{}", refused.text);
    }
    let parsed = Target::parse("L12:abc").unwrap();
    assert_eq!((parsed.line, parsed.hash), (12, "abc"));
}
