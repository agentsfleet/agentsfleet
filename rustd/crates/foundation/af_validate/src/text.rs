//! The one ceiling on text a person or a model hands the system: a setup
//! chat message, a message typed into a running fleet, a fleet's
//! instructions.
//!
//! Codex's server keeps the same number (`MAX_USER_INPUT_TEXT_CHARS = 1 << 20`
//! in `codex-rs/protocol/src/user_input.rs`) and refuses past it with the same
//! sentence. Below it, the limit a person meets is the model's context window,
//! which every model call already checks. Each field declares it with garde,
//! `length(chars, max = TEXT_MAX_CHARS)`, beside the field's own rules.

/// The most characters one text may hold, counted as `char`s: the unit a
/// published `maxLength` counts.
pub const TEXT_MAX_CHARS: usize = 1 << 20;

/// The refusal a text past [`TEXT_MAX_CHARS`] earns, naming the ceiling.
///
/// The number is written out because a constant's name means nothing to the
/// person reading the refusal; the test beside it pins the two together.
pub const TEXT_TOO_LONG: &str = "Input exceeds the maximum length of 1,048,576 characters.";

/// The widest one character serializes to in JSON: a `\u00XX` control escape.
const JSON_CHAR_MAX_BYTES: usize = 6;

/// The most bytes a text at [`TEXT_MAX_CHARS`] takes as a JSON string.
///
/// Every character at its widest escape: what a body carrying one such text
/// must be allowed, so the transport never refuses what the ceiling admits.
pub const TEXT_MAX_JSON_BYTES: usize = JSON_CHAR_MAX_BYTES * TEXT_MAX_CHARS;

#[cfg(test)]
mod tests {
    use super::{TEXT_MAX_CHARS, TEXT_MAX_JSON_BYTES, TEXT_TOO_LONG};

    #[test]
    fn the_ceiling_is_codexs_and_its_refusal_names_it() {
        assert_eq!(TEXT_MAX_CHARS, 1_048_576);
        assert_eq!(
            TEXT_TOO_LONG,
            "Input exceeds the maximum length of 1,048,576 characters."
        );
    }

    /// Every character a control escape is the widest a text serializes to:
    /// one such character is six bytes, and the bound is that times the ceiling.
    #[test]
    fn the_json_bound_holds_the_ceiling_at_its_widest_escape() {
        let widest = serde_json::to_string(&"\u{1}".repeat(TEXT_MAX_CHARS)).unwrap_or_default();
        assert_eq!(
            widest.len(),
            TEXT_MAX_JSON_BYTES + 2,
            "two quotes beside the text"
        );
        assert_eq!(TEXT_MAX_JSON_BYTES, 6_291_456);
    }
}
