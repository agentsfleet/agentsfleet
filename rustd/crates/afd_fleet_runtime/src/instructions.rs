//! The behaviour prose a lease carries, lifted out of a fleet's source markdown.
//!
//! # Why only this much of the markdown is read here
//!
//! `config_markdown.zig` turns `TRIGGER.md` into a `config_json` at INSTALL
//! time, and that half belongs to the tenant surface — it needs a YAML parser
//! and this milestone has no caller for one. What the LEASE path needs is
//! smaller and shares none of it: the prose after the frontmatter, byte for
//! byte, with no YAML read at all. So the delimiter scan is ported and the
//! parser is not.
//!
//! The prose is soft reasoning input. Hard tool and secret policy travels in
//! the execution policy, never here, so nothing downstream trusts these bytes
//! for a decision.

/// The whitespace the frontmatter scan trims.
///
/// Spelled as the four bytes rather than `char::is_whitespace`, which also
/// strips vertical tab, form feed and the Unicode spaces. Instructions are
/// compared against bytes the Zig produced, and a wider trim would silently
/// disagree on a document containing one of them.
const TRIMMED: [char; 4] = [' ', '\t', '\r', '\n'];

/// The frontmatter fence, opening and closing.
const FENCE: &str = "---";

/// The closing fence as it appears mid-document, at the start of its own line.
const CLOSING: &str = "\n---";

/// The markdown body that follows the YAML frontmatter.
///
/// Borrowed from `source_markdown`, so the lease's prose costs no copy on the
/// path every claim takes.
///
/// Empty when there is no well-formed frontmatter — which is a fleet whose
/// source was written without one, not a failure. It is not fallible for that
/// reason: there is no way for a caller to act differently on "no prose" than
/// on "empty prose", and a `Result` here would be a decision nobody can make.
#[must_use]
pub fn instructions(source_markdown: &str) -> &str {
    let trimmed = source_markdown.trim_matches(TRIMMED);
    let Some(after_open) = trimmed.strip_prefix(FENCE) else {
        return "";
    };
    let Some(close) = closing_fence(after_open) else {
        return "";
    };
    after_open
        .get(close + CLOSING.len()..)
        .unwrap_or_default()
        .trim_matches(TRIMMED)
}

/// Where the frontmatter closes, if it closes.
///
/// The fence must start a line AND end one. Without the second half, a YAML
/// value like `separator: ---bar` reads as the close and the rest of the
/// frontmatter becomes prose — so a match that is not followed by a line
/// ending or the end of input keeps searching rather than stopping.
fn closing_fence(haystack: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(offset) = haystack.get(from..)?.find(CLOSING) {
        let at = from + offset;
        let rest = haystack.get(at + CLOSING.len()..).unwrap_or_default();
        if rest.is_empty() || rest.starts_with('\n') || rest.starts_with('\r') {
            return Some(at);
        }
        from = at + 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::instructions;

    #[test]
    fn the_body_after_the_frontmatter_is_the_prose() {
        let source = "---\nname: probe\n---\nDo the thing.\n";

        assert_eq!(instructions(source), "Do the thing.");
    }

    #[test]
    fn a_document_with_no_frontmatter_has_no_prose() {
        // Not a failure — a fleet whose source was written without a
        // frontmatter block simply carries no instructions.
        assert_eq!(instructions("Just prose, no fence.\n"), "");
        assert_eq!(instructions(""), "");
    }

    #[test]
    fn an_unclosed_frontmatter_yields_nothing_rather_than_the_yaml() {
        // The alternative reading — treat the whole rest as prose — would ship
        // a fleet's YAML to the model as its behaviour.
        assert_eq!(instructions("---\nname: probe\nDo the thing.\n"), "");
    }

    #[test]
    fn a_fence_inside_a_yaml_value_does_not_close_the_block() {
        // `separator: ---x` starts a line with `---` only after a newline, and
        // is not followed by a line ending. Treating it as the close would
        // spill the rest of the frontmatter into the prose.
        let source = "---\nname: probe\n---x is not a fence\n---\nReal prose.\n";

        assert_eq!(instructions(source), "Real prose.");
    }

    #[test]
    fn surrounding_blank_lines_are_not_part_of_the_prose() {
        let source = "\n\n---\nname: probe\n---\n\n  Prose.  \n\n";

        assert_eq!(instructions(source), "Prose.");
    }

    #[test]
    fn a_frontmatter_with_no_body_yields_empty_prose() {
        assert_eq!(instructions("---\nname: probe\n---"), "");
        assert_eq!(instructions("---\nname: probe\n---\n"), "");
    }

    #[test]
    fn multi_byte_prose_survives_the_scan() {
        // Every slice above is taken at a byte offset; a boundary error here
        // would panic rather than return a wrong answer.
        let source = "---\nname: probe\n---\nRun the — dash — thing ✅\n";

        assert_eq!(instructions(source), "Run the — dash — thing ✅");
    }
}
