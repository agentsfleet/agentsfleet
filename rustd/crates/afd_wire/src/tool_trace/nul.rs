//! NUL, the one character no stored tool string may hold.
//!
//! Postgres refuses a NUL byte in `text` and the `\u0000` escape in `jsonb`.
//! Tool output can carry one — a binary file read, a `-print0` listing — and a
//! statement binding it fails whole, so the strings are checked before they
//! reach one.

use serde_json::{Map, Value};

/// Whether `text` holds no NUL character.
///
/// Postgres refuses NUL in `text` and `\u0000` in `jsonb`, so a string that
/// carries one cannot be stored, and a statement binding it fails whole.
#[must_use]
pub fn free_of_nul(text: &str) -> bool {
    !text.contains(NUL)
}

/// Whether every key and string inside `fields` is free of NUL.
#[must_use]
pub fn fields_free_of_nul(fields: &Map<String, Value>) -> bool {
    fields
        .iter()
        .all(|(key, value)| free_of_nul(key) && value_free_of_nul(value))
}

/// Whether every key and string inside `value` is free of NUL.
fn value_free_of_nul(value: &Value) -> bool {
    match value {
        Value::String(text) => free_of_nul(text),
        Value::Array(items) => items.iter().all(value_free_of_nul),
        Value::Object(fields) => fields_free_of_nul(fields),
        Value::Null | Value::Bool(_) | Value::Number(_) => true,
    }
}

/// The character no stored string may hold.
const NUL: char = '\0';
