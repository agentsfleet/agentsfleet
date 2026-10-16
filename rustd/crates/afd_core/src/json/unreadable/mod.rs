//! A `serde_json` refusal with the refused value taken out.
//!
//! `serde` renders the value it refused into its message — `invalid type:
//! string "# private instructions…", expected struct Definition` — so a setup
//! sent in the wrong shape would carry its author's private text into every log
//! line and error chain that renders the cause. [`unknown_field_of`] answers
//! that for one refusal; this answers it for every refusal a document can draw.
//!
//! What stays is what an author needs to find the fault and what the document
//! did not supply: the position, the KIND of value found (`string`, never the
//! string), and serde's `expected …` clause, which the type being read words.
//! A field in `missing field` or `duplicate field` is the schema's name, so it
//! stays; a key in `unknown field` is the author's, so it stays only inside the
//! bound [`unknown_field_of`] already applies. A name in `unknown variant` is a
//! value and goes. A message this does not recognise — a refusal some
//! `Deserialize` impl worded itself — goes whole rather than being guessed at.
//!
//! Syntax and end-of-input refusals keep serde's text: their messages are
//! `serde_json`'s fixed vocabulary, never a quote of the input.

use std::fmt;

use serde_json::error::Category;

use super::unknown_field_of;

/// What replaces a refusal this module does not recognise.
const UNRECOGNISED: &str = "a value is not accepted";
/// What an input/output failure reads as; its cause is the reader, not the text.
const IO_FAILURE: &str = "the document could not be read";
/// What `serde_json` appends to a positioned message.
const POSITION: &str = " at line ";
/// What separates the refused value from the type that was expected.
const EXPECTED: &str = ", expected ";
/// The refusals whose value is replaced by its kind.
const WRONG_SHAPE: [&str; 2] = ["invalid type: ", "invalid value: "];
/// The refusals that name a field from the schema rather than the document.
const SCHEMA_FIELD: [&str; 2] = ["missing field `", "duplicate field `"];
/// The refusal that counts elements; the count is metadata, not content.
const WRONG_LENGTH: &str = "invalid length ";
/// The refusal that names a variant the document supplied.
const UNKNOWN_VARIANT: &str = "unknown variant";
/// The refusal that names a key the document supplied.
const UNKNOWN_FIELD: &str = "unknown field";
/// Longest `expected …` clause kept; past it the wording is not a type's.
const EXPECTED_MAX_CHARS: usize = 256;
/// What a found value reads as when serde named a kind this does not list.
const ANY_VALUE: &str = "a value";
/// `serde::de::Unexpected`'s kinds, longest first so `unit variant` wins over
/// a shorter prefix.
const VALUE_KINDS: [&str; 16] = [
    "newtype variant",
    "struct variant",
    "floating point",
    "newtype struct",
    "tuple variant",
    "Option value",
    "unit variant",
    "byte array",
    "unit value",
    "character",
    "sequence",
    "boolean",
    "integer",
    "string",
    "enum",
    "map",
];

/// A `serde_json` refusal that renders its position and the expected type but
/// none of the document's own text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unreadable {
    category: Category,
    line: usize,
    column: usize,
    what: Box<str>,
}

impl Unreadable {
    /// Which kind of refusal this was: syntax, data, end of input, or I/O.
    #[must_use]
    pub const fn category(&self) -> Category {
        self.category
    }

    /// The 1-based line serde stopped on; 0 when it reported no position.
    #[must_use]
    pub const fn line(&self) -> usize {
        self.line
    }

    /// The 1-based column serde stopped on; 0 when it reported no position.
    #[must_use]
    pub const fn column(&self) -> usize {
        self.column
    }
}

impl From<serde_json::Error> for Unreadable {
    fn from(error: serde_json::Error) -> Self {
        let rendered = error.to_string();
        let message = match rendered.rfind(POSITION) {
            Some(at) if error.line() != 0 => &rendered[..at],
            _ => rendered.as_str(),
        };
        let what = match error.classify() {
            Category::Io => IO_FAILURE.into(),
            Category::Syntax | Category::Eof => message.into(),
            Category::Data => data(message, &error),
        };
        Self {
            category: error.classify(),
            line: error.line(),
            column: error.column(),
            what,
        }
    }
}

impl fmt::Display for Unreadable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            return f.write_str(&self.what);
        }
        write!(
            f,
            "{}{POSITION}{} column {}",
            self.what, self.line, self.column
        )
    }
}

impl std::error::Error for Unreadable {}

/// A data refusal reworded without the value the document supplied.
fn data(message: &str, error: &serde_json::Error) -> Box<str> {
    let expected = expected(message);
    if let Some(found) = WRONG_SHAPE
        .iter()
        .find_map(|prefix| message.strip_prefix(prefix).map(|rest| (prefix, rest)))
    {
        let (prefix, rest) = found;
        let kind = VALUE_KINDS
            .iter()
            .find(|kind| rest.starts_with(*kind))
            .copied()
            .unwrap_or(ANY_VALUE);
        return with_expected(&format!("{prefix}{kind}"), expected);
    }
    if SCHEMA_FIELD
        .iter()
        .any(|prefix| message.starts_with(prefix))
    {
        return message.into();
    }
    if let Some(rest) = message.strip_prefix(WRONG_LENGTH) {
        let count: String = rest.chars().take_while(char::is_ascii_digit).collect();
        return with_expected(&format!("{WRONG_LENGTH}{count}"), expected);
    }
    if message.starts_with(UNKNOWN_VARIANT) {
        return with_expected(UNKNOWN_VARIANT, expected);
    }
    if message.starts_with(UNKNOWN_FIELD) {
        let named = unknown_field_of(error).map_or_else(
            || UNKNOWN_FIELD.to_owned(),
            |name| format!("{UNKNOWN_FIELD} `{name}`"),
        );
        return with_expected(&named, expected);
    }
    UNRECOGNISED.into()
}

/// serde's `expected …` clause, which the type being read words, when it is
/// short enough to be a type's wording.
///
/// The LAST separator is taken because a value quoted earlier in the message
/// may contain the separator itself; serde's own comes after the value.
fn expected(message: &str) -> Option<&str> {
    let (_, clause) = message.rsplit_once(EXPECTED)?;
    (clause.chars().count() <= EXPECTED_MAX_CHARS).then_some(clause)
}

fn with_expected(head: &str, expected: Option<&str>) -> Box<str> {
    match expected {
        Some(clause) => format!("{head}{EXPECTED}{clause}").into(),
        None => head.into(),
    }
}

#[cfg(test)]
mod tests;
