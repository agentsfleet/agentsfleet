//! The byte bound on a setup Definition as JSON text.
//!
//! The Definition parser (`af_fleet_runtime`) and the runner's setup report
//! (`af_wire`) both bound the same document. `af_wire` cannot depend on
//! `af_fleet_runtime`, so the arithmetic lives here and both read it. One
//! formula means a report never refuses a Definition the parser admits.

use crate::text::{JSON_CHAR_MAX_BYTES, TEXT_MAX_CHARS};

/// The most characters a Definition's description may hold.
pub const DESCRIPTION_MAX_CHARS: usize = 32 * 1024;

/// Room the configuration takes in a Definition beside its two texts.
const CONFIG_ALLOWANCE_BYTES: usize = 64 * 1024;

/// The most bytes a Definition, or an edit to one, takes as JSON text.
///
/// The description and the instructions at their character caps, every
/// character at its widest JSON escape, plus the configuration.
pub const DEFINITION_MAX_BYTES: usize =
    JSON_CHAR_MAX_BYTES * (DESCRIPTION_MAX_CHARS + TEXT_MAX_CHARS) + CONFIG_ALLOWANCE_BYTES;

#[cfg(test)]
mod tests {
    use super::{DEFINITION_MAX_BYTES, DESCRIPTION_MAX_CHARS};
    use crate::TEXT_MAX_CHARS;

    /// The CLI pins the same number (`cli/test/chat/definition.test.ts`).
    #[test]
    fn the_bound_is_both_texts_at_their_widest_plus_the_configuration() {
        assert_eq!(DEFINITION_MAX_BYTES, 6_553_600);
        assert_eq!(
            DEFINITION_MAX_BYTES,
            6 * (DESCRIPTION_MAX_CHARS + TEXT_MAX_CHARS) + 65_536
        );
    }

    /// Both texts at their caps, every character a control escape, fit.
    #[test]
    fn a_definition_with_both_texts_at_their_widest_fits() {
        let definition = serde_json::json!({
            "schema_version": 1,
            "description": "\u{1}".repeat(DESCRIPTION_MAX_CHARS),
            "instructions_md": "\u{1}".repeat(TEXT_MAX_CHARS),
            "config": {"name": "maximal"},
        });
        let text = serde_json::to_string(&definition).unwrap_or_default();
        assert!(text.len() <= DEFINITION_MAX_BYTES, "{} bytes", text.len());
    }
}
