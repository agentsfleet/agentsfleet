//! NUL, the one character no stored tool string may hold.
//!
//! Postgres refuses a NUL byte in `text` and the `\u0000` escape in `jsonb`.
//! Tool output can carry one — a binary file read, a `-print0` listing — and a
//! statement binding it fails whole, so the strings are checked before they
//! reach one.

use serde_json::{Map, Value};

/// Whether `text` holds no NUL character.
///
/// `afd_validate::nul_free`'s answer as a yes or no, because a trace's rules
/// fold every string one call carries into one report rather than one per
/// field.
#[must_use]
pub fn free_of_nul(text: &str) -> bool {
    afd_validate::nul_free(text, &()).is_ok()
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
