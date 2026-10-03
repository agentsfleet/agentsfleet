//! Reading a JSON OBJECT into a type, and refusing anything that is not one.
//!
//! # The hole this closes
//!
//! `serde_json` fills a derived struct from a JSON ARRAY, taking its elements
//! POSITIONALLY. `["anthropic","sk-live"]` deserializes into a two-field
//! credential as happily as `{"provider":…,"api_key":…}` does, and every shape
//! check after it passes, because the value really does have both fields.
//!
//! That is by design and it is not a serde defect: the derive implements
//! `visit_seq` alongside `visit_map` so one type can ride a self-describing
//! format and a compact one. It is a hole only where the JSON came from outside
//! this process, because every contract this daemon publishes says object.
//!
//! `#[serde(deny_unknown_fields)]` does NOT close it — an array has no field
//! names to be unknown.
//!
//! # How it is closed: by telling serde, not by reading bytes
//!
//! The derive asks the deserializer for a STRUCT, and `serde_json` answers that
//! request by peeking at the next token and accepting either `{` or `[`.
//! [`ObjectOnly`] sits between the two and forwards that one request as a
//! request for a MAP instead — which `serde_json` answers with `{` alone.
//!
//! ```text
//!   derive          adapter                serde_json          input
//!   ──────          ───────                ──────────          ─────
//!   deserialize_struct ─► deserialize_map ─► expects `{`  ◄──  {"a":1}   ✓
//!                                                         ◄──  [1]       ✗
//!                                                              "invalid type:
//!                                                               sequence,
//!                                                               expected struct"
//! ```
//!
//! One pass over the bytes, no intermediate tree, and the refusal is serde's
//! own — so it names the type that was expected instead of a generic complaint
//! this module would have had to word itself.
//!
//! Everything else forwards untouched, so a missing field, a wrong type and a
//! borrowed `&'de str` all behave exactly as they do without the adapter. Only
//! the TOP level is constrained, which is the same scope `loadJson`'s
//! `parsed.value != .object` has.

use serde::de::{Deserializer, Visitor};
use serde::forward_to_deserialize_any;

/// A deserializer that will not read a struct out of a sequence.
///
/// Every method forwards to the wrapped deserializer. The single exception is
/// [`Deserializer::deserialize_struct`], which forwards as
/// [`Deserializer::deserialize_map`] — so the format decides what a map is, and
/// this decides only that a struct must be one.
struct ObjectOnly<D>(D);

impl<'de, D: Deserializer<'de>> Deserializer<'de> for ObjectOnly<D> {
    type Error = D::Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.0.deserialize_any(visitor)
    }

    /// The one redirection: a struct is a map, never a sequence.
    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.0.deserialize_map(visitor)
    }

    // A self-describing format answers all of these from the value it finds, so
    // routing them through `deserialize_any` changes nothing about how they
    // read — which is what makes this adapter one redirection rather than a
    // re-implementation of the trait.
    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map enum identifier ignored_any
    }
}

/// Reads an object from an existing Serde deserializer without an intermediate tree.
///
/// # Errors
/// Returns the format's error for a non-object or invalid field.
pub fn object_from_deserializer<'de, T, D>(format: D) -> Result<T, D::Error>
where
    T: serde::Deserialize<'de>,
    D: Deserializer<'de>,
{
    T::deserialize(ObjectOnly(format))
}

/// Deserializes `body` into `T`, refusing any JSON that is not an object.
///
/// A drop-in for [`serde_json::from_slice`] at a trust boundary: same
/// signature, same error type, so a call site keeps whatever it already does
/// with the failure. Borrowing types are supported — `body` outlives the result
/// — so a `#[serde(borrow)]` request shape reads through this unchanged.
///
/// # Errors
/// Returns `serde_json`'s own error: `invalid type: sequence, expected struct
/// …` when the body is an array, and whatever it reports for JSON that does not
/// fit `T`.
pub fn object_from_slice<'de, T>(body: &'de [u8]) -> Result<T, serde_json::Error>
where
    T: serde::Deserialize<'de>,
{
    let mut format = serde_json::Deserializer::from_slice(body);
    let value = object_from_deserializer(&mut format)?;
    // What `from_slice` does after its own parse: refuse trailing bytes, so
    // `{} garbage` is not silently half-read.
    format.end()?;
    Ok(value)
}

/// [`object_from_slice`], refusing any field `T` would have ignored, at any depth.
///
/// # Why a reader rather than `#[serde(deny_unknown_fields)]`
///
/// A type the runner reads stays lenient, so a daemon that grows a field never
/// strands a runner built before it. Some of those types are also embedded in a
/// request a PERSON writes — an operator's assigned policy, an enrolment — and
/// there a misspelled key must still be refused rather than dropped. The rule
/// belongs to the boundary that reads the body, not to the type, so the
/// boundary chooses this reader. `serde_ignored` reports every key the derive
/// skipped, with its dotted path; the first one becomes serde's own
/// unknown-field refusal, so [`unknown_field_of`] names it exactly as it names
/// a refusal from a closed struct.
///
/// # Errors
/// Everything [`object_from_slice`] refuses, plus `unknown field` for the first
/// ignored key, path included — `policy.wroker_count`, or
/// `assigned_policy.?.wroker_count` where `?` is `serde_ignored`'s spelling of
/// the `Option` the key sat inside.
pub fn strict_object_from_slice<'de, T>(body: &'de [u8]) -> Result<T, serde_json::Error>
where
    T: serde::Deserialize<'de>,
{
    let mut format = serde_json::Deserializer::from_slice(body);
    let mut ignored = None;
    let value = serde_ignored::deserialize(ObjectOnly(&mut format), |path| {
        ignored.get_or_insert_with(|| path.to_string());
    })?;
    format.end()?;
    match ignored {
        Some(path) => Err(serde::de::Error::unknown_field(&path, &[])),
        None => Ok(value),
    }
}

/// The field name from an `unknown field` refusal, when that is what failed.
///
/// # Why a whitelist rather than logging the error
///
/// `serde_json::Error` renders two very different things through one `Display`.
/// An unknown-field refusal names only NAMES — it renders the offending key and
/// the ones it expected, nothing else — and is safe to log. A type
/// refusal embeds the VALUE it rejected — `invalid type: string "sk-live-…",
/// expected u32` — and these bodies carry api keys, minted tokens and secret
/// maps, so logging one raw would put a credential in the log.
///
/// So the error text is never logged. This reads the one shape that is safe,
/// returns the bare field name, and answers `None` for everything else, which
/// keeps the caller's generic refusal for every other failure.
///
/// # Why the message is parsed at all
///
/// `serde` exposes no structured accessor for the offending field —
/// `Error::classify` answers `Data` for both refusals above and the field lives
/// only in the rendered string. The prefix and the backtick delimiters are
/// `serde`'s own and stable across the 1.x line; a render this does not
/// recognise answers `None` rather than guessing, so a future change degrades
/// to today's behaviour instead of logging something unexpected.
///
/// The name is bounded because it is attacker-chosen: a caller controls the
/// keys it sends, and an unbounded one would put a megabyte in a log line the
/// standard caps at 300 characters.
#[must_use]
pub fn unknown_field_of(error: &serde_json::Error) -> Option<String> {
    /// Longest field name reported. Past this the caller chose the name to fill
    /// a log, not to name a field.
    const NAME_MAX_CHARS: usize = 64;
    /// What `serde` opens an unknown-field refusal with.
    const PREFIX: &str = "unknown field `";

    if error.classify() != serde_json::error::Category::Data {
        return None;
    }
    let rendered = error.to_string();
    let after = rendered.strip_prefix(PREFIX)?;
    let name = after.split('`').next()?;
    if name.is_empty() || name.chars().count() > NAME_MAX_CHARS {
        return None;
    }
    Some(name.to_owned())
}

#[cfg(test)]
#[path = "json/tests.rs"]
mod tests;
