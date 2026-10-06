//! Hashline's tags on text alone: the hash, the lines it is taken over, and
//! the radius a tag is looked for in.

use super::{Line, Located, Target, lines, tag, tagged};

/// The tag is nullclaw's: Fowler–Noll–Vo 1a over the trimmed line before,
/// a bar, and the trimmed line, keeping twelve bits as three hex digits.
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
