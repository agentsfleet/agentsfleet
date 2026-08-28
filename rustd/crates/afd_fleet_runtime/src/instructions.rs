//! The behaviour prose a lease carries, lifted out of a fleet's source markdown.
//!
//! # Why the lease path reads only this half
//!
//! [`crate::frontmatter`] splits an authored document into its YAML block and
//! its prose, and the two halves have entirely separate readers. Install and
//! config-PATCH take the YAML and turn it into `config_json`; the LEASE path
//! takes the prose, byte for byte, and reads no YAML at all. This module is
//! that second reader, and it stays its own name because "what a lease
//! carries" is a question worth answering in one function.
//!
//! M177 ported the delimiter scan alone, because the lease path was the only
//! caller a runner-plane milestone had. The YAML half arrived with the tenant
//! surface that installs fleets; both now share one scan, so the fence rules
//! cannot drift between the daemon that stores a document and the daemon that
//! runs it.
//!
//! The prose is soft reasoning input. Hard tool and secret policy travels in
//! the execution policy, never here, so nothing downstream trusts these bytes
//! for a decision.

use crate::frontmatter;

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
    frontmatter::scan(source_markdown).map_or("", |block| block.body())
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
