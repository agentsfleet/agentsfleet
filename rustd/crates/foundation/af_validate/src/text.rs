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

/// The widest one character serializes to when this system writes JSON: a
/// `\u00XX` control escape. serde writes every other character as itself.
pub(crate) const JSON_CHAR_MAX_BYTES: usize = 6;

/// The widest one character can arrive in JSON a client wrote: a character
/// outside the Basic Multilingual Plane, escaped as a surrogate pair
/// (`\uD83D\uDE00`), which is one `char` against the ceiling.
pub(crate) const JSON_INPUT_CHAR_MAX_BYTES: usize = 12;

/// The most bytes a text at [`TEXT_MAX_CHARS`] takes as a JSON string a
/// client wrote.
///
/// Every character at the widest escape a client may send: what a body
/// carrying one such text must be allowed, so the transport never refuses
/// what the ceiling admits, and a text past the ceiling is refused with
/// [`TEXT_TOO_LONG`] rather than for its body's size.
pub const TEXT_MAX_JSON_BYTES: usize = JSON_INPUT_CHAR_MAX_BYTES * TEXT_MAX_CHARS;

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

    /// A control character is the widest serde writes, six bytes; an astral
    /// character escaped as a surrogate pair is the widest a client can
    /// send, twelve bytes for one character. The bound is the client's.
    #[test]
    fn the_json_bound_holds_the_ceiling_at_a_clients_widest_escape() {
        let written = serde_json::to_string(&"\u{1}".repeat(TEXT_MAX_CHARS)).unwrap_or_default();
        assert_eq!(
            written.len(),
            super::JSON_CHAR_MAX_BYTES * TEXT_MAX_CHARS + 2
        );
        let sent = format!("\"{}\"", r"\uD83D\uDE00".repeat(TEXT_MAX_CHARS));
        assert_eq!(
            sent.len(),
            TEXT_MAX_JSON_BYTES + 2,
            "two quotes beside the text"
        );
        let read: String = serde_json::from_str(&sent).unwrap_or_default();
        assert_eq!(read.chars().count(), TEXT_MAX_CHARS, "one character each");
        assert_eq!(TEXT_MAX_JSON_BYTES, 12_582_912);
    }
}
