//! Rendering a stream field as text, and the sample shapes that prove it.
//!
//! Split from [`super`] on the line between the stream's commands and what a
//! field VALUE becomes on the way out: nothing here issues a command, and the
//! one function that matters is called on every field of every read.

/// Every reply shape [`stringify`] renders, each with the label it is rendered
/// from.
///
/// Exposed under `test-util` because Redis will not produce these on demand: a
/// stream field is a bulk string on the wire, so the arms that keep a
/// surprising value readable have no other way to be reached. A producer that
/// starts writing something else — or a redis-rs release that decodes an
/// integer field differently — is exactly the surprise these arms exist for,
/// and an unrendered one reaching a caller as an empty string is silent.
#[cfg(feature = "test-util")]
#[must_use]
pub fn rendered_field_samples() -> Vec<(&'static str, String)> {
    vec![
        (
            "bulk string",
            stringify(&redis::Value::BulkString(b"ready".to_vec())),
        ),
        (
            "simple string",
            stringify(&redis::Value::SimpleString("OK".to_owned())),
        ),
        ("integer", stringify(&redis::Value::Int(42))),
        ("anything else", stringify(&redis::Value::Nil)),
        (
            "invalid utf-8",
            stringify(&redis::Value::BulkString(vec![0xff, 0xfe])),
        ),
    ]
}

/// Renders a stream field value as text.
///
/// Stream fields are byte strings on the wire. Anything else is a value this
/// producer did not write, and rendering it through `Debug` keeps a surprising
/// entry readable instead of failing the whole read.
///
/// # The crate's own conversion, with the fallback this caller needs
///
/// `String::from_redis_value_ref` is the redis crate's answer to "render this
/// reply as text", and it knows more shapes than a hand-written match will keep
/// up with: `Okay`, `VerbatimString` and `Double` on top of the three below,
/// and it unwraps an attribute-wrapped value before looking. Re-deciding that
/// here is a second copy of the crate's knowledge that drifts every release.
///
/// What it does NOT do is stay infallible: it errors on a value that is not
/// string-compatible, and on a bulk string that is not UTF-8. This caller
/// cannot use an error — a single surprising field would fail an entire stream
/// read — so the conversion is composed with the `Debug` fallback rather than
/// replaced by it.
///
/// One behaviour changed with this: a bulk string carrying invalid UTF-8 used
/// to render lossily, with replacement characters, and now renders as
/// `binary-data([..])`. That is the better answer of the two. A field this
/// daemon wrote is always valid UTF-8, so invalid bytes mean a foreign
/// producer, and a reader chasing that wants the bytes rather than a sentence
/// with question marks punched through it.
pub(crate) fn stringify(value: &redis::Value) -> String {
    redis::FromRedisValue::from_redis_value_ref(value)
        .unwrap_or_else(|_not_text| format!("{value:?}"))
}
