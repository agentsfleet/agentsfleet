//! The byte bound on a setup Definition as JSON text.
//!
//! The Definition parser (`af_fleet_runtime`) and the runner's setup report
//! (`af_wire`) both bound the same document. `af_wire` cannot depend on
//! `af_fleet_runtime`, so the arithmetic lives here and both read it. One
//! formula means a report never refuses a Definition the parser admits.

use crate::text::{JSON_CHAR_MAX_BYTES, JSON_INPUT_CHAR_MAX_BYTES, TEXT_MAX_CHARS};

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

/// The most bytes a Definition at its caps takes as JSON a client wrote.
///
/// [`DEFINITION_MAX_BYTES`] bounds the document as this system re-serializes
/// it; a request body carries it as the client wrote it, where one astral
/// character may be a twelve-byte surrogate-pair escape. A body limit reads
/// this one, so both texts at their caps reach the parser and its refusals.
pub const DEFINITION_MAX_JSON_BYTES: usize =
    JSON_INPUT_CHAR_MAX_BYTES * (DESCRIPTION_MAX_CHARS + TEXT_MAX_CHARS) + CONFIG_ALLOWANCE_BYTES;

#[cfg(test)]
mod tests {
    use super::{DEFINITION_MAX_BYTES, DEFINITION_MAX_JSON_BYTES, DESCRIPTION_MAX_CHARS};
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

    /// Both texts at their caps, every character an escaped astral one, fit
    /// the bound a body carrying the Definition reads.
    #[test]
    fn a_definition_a_client_escaped_at_its_widest_fits_the_body_bound() {
        let astral = |chars: usize| r"\uD83D\uDE00".repeat(chars);
        let sent = format!(
            r#"{{"schema_version":1,"description":"{}","instructions_md":"{}","config":{{"name":"maximal"}}}}"#,
            astral(DESCRIPTION_MAX_CHARS),
            astral(TEXT_MAX_CHARS),
        );
        assert!(sent.len() > DEFINITION_MAX_BYTES, "{} bytes", sent.len());
        assert!(
            sent.len() <= DEFINITION_MAX_JSON_BYTES,
            "{} bytes",
            sent.len()
        );
        assert_eq!(DEFINITION_MAX_JSON_BYTES, 13_041_664);
    }
}
