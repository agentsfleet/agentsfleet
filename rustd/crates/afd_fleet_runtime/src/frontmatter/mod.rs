//! The `---` fenced block at the head of an authored document.
//!
//! # The two halves, and who reads which
//!
//! An authored `TRIGGER.md` or `SKILL.md` is a YAML block between two fences
//! followed by markdown prose. The halves go to different readers and neither
//! one wants the other: [`crate::instructions`] takes the PROSE on the lease
//! path and reads no YAML at all, while install and config-PATCH take the YAML
//! and discard the prose. One scan answers both, which is the whole reason this
//! module exists rather than a second delimiter walk beside each caller.
//!
//! # Why the fence rules are this fussy
//!
//! `config_markdown.zig`'s `findClosingDelim` requires the closing `---` to
//! START a line and to END one — the match must be preceded by `\n` and
//! followed by `\n`, `\r`, or the end of input. Both halves are load-bearing
//! and each has a document that proves it:
//!
//! - Without the first, `separator: ---bar` inside a YAML value reads as the
//!   close and the rest of the frontmatter spills into the prose.
//! - Without the second, a line reading `---x is not a fence` closes the block
//!   early.
//!
//! A consequence worth knowing before reaching for a regular expression: a
//! closing fence with TRAILING SPACES (`"---   \n"`) does not close, because
//! the byte after the fence is a space rather than a line ending. That is
//! stricter than most frontmatter readers and it is deliberate — a permissive
//! pre-split would accept documents the Zig daemon refuses, which is a parity
//! break in the direction nobody notices until a fleet installs on one daemon
//! and not the other.

pub mod json;
pub mod skill;
pub mod trigger;

pub use self::skill::{SkillMetadata, parse_skill};
pub use self::trigger::{ParsedTrigger, parse_trigger};

/// The whitespace the frontmatter scan trims.
///
/// Spelled as the four bytes rather than `char::is_whitespace`, which also
/// strips vertical tab, form feed and the Unicode spaces. Instructions are
/// compared against bytes the Zig produced, and a wider trim would silently
/// disagree on a document containing one of them.
pub(crate) const TRIMMED: [char; 4] = [' ', '\t', '\r', '\n'];

/// The frontmatter fence, opening and closing.
const FENCE: &str = "---";

/// The closing fence as it appears mid-document, at the start of its own line.
const CLOSING: &str = "\n---";

/// A document's frontmatter block and the prose beneath it.
///
/// Both halves borrow the document, so a scan costs no copy on the lease path
/// that takes only the prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frontmatter<'doc> {
    yaml: &'doc str,
    body: &'doc str,
}

impl<'doc> Frontmatter<'doc> {
    /// The YAML between the fences, untrimmed.
    ///
    /// Carries the newline that follows the opening fence, exactly as
    /// `scanFrontmatter` slices it. Leading blank space is a YAML no-op, and
    /// trimming it here would be a second place for the two daemons to
    /// disagree about what the parser was handed.
    #[must_use]
    pub const fn yaml(self) -> &'doc str {
        self.yaml
    }

    /// The markdown prose after the closing fence, trimmed.
    #[must_use]
    pub const fn body(self) -> &'doc str {
        self.body
    }
}

/// Splits a document at its frontmatter fences.
///
/// [`None`] when there is no well-formed block — no opening fence, or an
/// opening fence that never closes. Absence is not a failure here: a document
/// authored without frontmatter is a document with no configuration to read,
/// and the callers that NEED one raise that themselves with a sentence about
/// the key they wanted.
#[must_use]
pub fn scan(document: &str) -> Option<Frontmatter<'_>> {
    let trimmed = document.trim_matches(TRIMMED);
    let after_open = trimmed.strip_prefix(FENCE)?;
    let close = closing_fence(after_open)?;
    Some(Frontmatter {
        yaml: after_open.get(..close)?,
        body: after_open
            .get(close + CLOSING.len()..)
            .unwrap_or_default()
            .trim_matches(TRIMMED),
    })
}

/// Where the frontmatter closes, if it closes.
///
/// The fence must start a line AND end one. Without the second half, a YAML
/// value like `separator: ---bar` reads as the close and the rest of the
/// frontmatter becomes prose — so a match that is not followed by a line
/// ending or the end of input keeps searching rather than stopping.
pub(crate) fn closing_fence(haystack: &str) -> Option<usize> {
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
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::scan;

    #[test]
    fn both_halves_come_back_from_one_scan() {
        let found = scan("---\nname: probe\n---\nDo the thing.\n").expect("a fenced document");

        assert_eq!(found.yaml(), "\nname: probe");
        assert_eq!(found.body(), "Do the thing.");
    }

    #[test]
    fn a_document_with_no_fence_scans_to_nothing() {
        assert!(scan("Just prose.\n").is_none());
        assert!(scan("").is_none());
    }

    #[test]
    fn an_unclosed_block_scans_to_nothing_rather_than_to_its_yaml() {
        // The alternative reading — treat everything after the opening fence
        // as the block — would hand a half-written document to the parser and
        // install whatever happened to parse.
        assert!(scan("---\nname: probe\nDo the thing.\n").is_none());
    }

    #[test]
    fn a_fence_inside_a_yaml_value_does_not_close_the_block() {
        let found = scan("---\nsep: ---x\n---\nProse.\n").expect("a fenced document");

        assert_eq!(found.yaml(), "\nsep: ---x");
        assert_eq!(found.body(), "Prose.");
    }

    #[test]
    fn a_closing_fence_with_trailing_spaces_does_not_close() {
        // `findClosingDelim` admits only `\n`, `\r` or end-of-input after the
        // fence. Pinned because a permissive pre-split is the obvious
        // "simplification" and it changes which documents install.
        assert!(scan("---\nname: probe\n---   \nProse.\n").is_none());
    }

    #[test]
    fn a_four_dash_line_does_not_close() {
        // `rest[0]` is `-`, so the scan keeps looking and finds no close.
        assert!(scan("---\nname: probe\n----\nProse.\n").is_none());
    }

    #[test]
    fn no_newline_after_the_opening_fence_still_opens() {
        // `scanFrontmatter` requires only the `---` prefix, so the YAML slice
        // can begin on the fence's own line.
        let found = scan("---name: probe\n---\n").expect("a fenced document");

        assert_eq!(found.yaml(), "name: probe");
        assert_eq!(found.body(), "");
    }

    #[test]
    fn multi_byte_prose_survives_the_scan() {
        // Every slice here is taken at a byte offset; a boundary error would
        // panic rather than return a wrong answer.
        let found = scan("---\nname: probe\n---\nRun the — dash — thing ✅\n").expect("fenced");

        assert_eq!(found.body(), "Run the — dash — thing ✅");
    }
}
