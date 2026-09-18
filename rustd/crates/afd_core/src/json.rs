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
mod tests {
    #![expect(
        clippy::expect_used,
        clippy::unwrap_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::object_from_slice;
    use serde::Deserialize;

    /// Two fields, which is what makes the positional reading reachable at all.
    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct Pair {
        provider: String,
        api_key: String,
    }

    /// A closed struct, so an unknown key is a refusal rather than ignored.
    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Closed {
        // Deserialized but never read: the refusal is the subject, not the value.
        #[expect(
            dead_code,
            reason = "the field exists so an unknown SIBLING is refusable"
        )]
        provider: String,
    }

    /// The field name comes back, so a log can say WHICH key was refused.
    #[test]
    fn an_unknown_field_refusal_yields_its_field_name() {
        let error = object_from_slice::<Closed>(br#"{"provider":"a","extra":1}"#)
            .expect_err("a closed struct refuses an unknown key");

        assert_eq!(super::unknown_field_of(&error).as_deref(), Some("extra"));
    }

    /// A type refusal yields NOTHING, because its rendering embeds the value it
    /// rejected — here an api key — and these bodies carry credentials.
    #[test]
    fn a_type_refusal_yields_nothing_so_no_value_can_reach_a_log() {
        let error = object_from_slice::<Pair>(br#"{"provider":1,"api_key":"sk-live-secret"}"#)
            .expect_err("a number is not a string");

        assert!(
            error.to_string().contains('1'),
            "precondition: serde embeds the rejected value"
        );
        assert_eq!(super::unknown_field_of(&error), None);
    }

    /// Syntax and EOF refusals are not `Data`, so they answer `None` too.
    #[test]
    fn a_malformed_body_yields_nothing() {
        let syntax = object_from_slice::<Pair>(b"{").expect_err("truncated");

        assert_eq!(super::unknown_field_of(&syntax), None);
    }

    /// An attacker-chosen name is bounded, because the log line that carries it
    /// is bounded and the sender picks the key.
    #[test]
    fn an_absurdly_long_field_name_is_refused_rather_than_logged() {
        let name = "z".repeat(4096);
        let body = format!(r#"{{"provider":"a","{name}":1}}"#);
        let error =
            object_from_slice::<Closed>(body.as_bytes()).expect_err("a closed struct refuses it");

        assert_eq!(super::unknown_field_of(&error), None);
    }

    /// The empty key is refused too, so a log line never carries a bare name.
    ///
    /// `serde` renders this one as ``unknown field ` ` `` with nothing between
    /// the delimiters, which parses cleanly and yields a name that says nothing.
    /// Answering `None` keeps the caller's generic refusal rather than logging
    /// `field=""`, which reads as a bug in the daemon rather than a bad request.
    #[test]
    fn the_empty_field_name_is_refused_rather_than_logged() {
        let error = object_from_slice::<Closed>(br#"{"provider":"a","":1}"#)
            .expect_err("a closed struct refuses the empty key like any other");

        assert_eq!(super::unknown_field_of(&error), None);
    }

    /// A borrowing shape, so the lifetime the request handlers need is proven
    /// rather than assumed.
    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct Borrowed<'a> {
        #[serde(borrow)]
        host_id: &'a str,
    }

    #[test]
    fn an_object_deserializes_exactly_as_serde_json_would() {
        let parsed: Pair =
            object_from_slice(br#"{"provider":"anthropic","api_key":"sk-live"}"#).unwrap();

        assert_eq!(
            parsed,
            Pair {
                provider: "anthropic".to_owned(),
                api_key: "sk-live".to_owned(),
            }
        );
    }

    #[test]
    fn a_positional_array_is_refused_where_serde_json_accepts_it() {
        // The hole, stated as the contrast: the plain call succeeds and fills
        // both fields in declaration order.
        let through_serde: Pair = serde_json::from_slice(br#"["anthropic","sk-live"]"#).unwrap();
        assert_eq!(through_serde.api_key, "sk-live");

        let refused = object_from_slice::<Pair>(br#"["anthropic","sk-live"]"#)
            .expect_err("an array is not an object, however well it lines up");
        // serde's own diagnosis, which names the type it wanted — a message
        // this module could not have written as well itself.
        assert!(
            refused.to_string().contains("invalid type: sequence"),
            "{refused}"
        );
        assert!(refused.to_string().contains("Pair"), "{refused}");
    }

    #[test]
    fn every_other_json_value_is_refused_too() {
        for refused in [
            br#""a string""#.as_slice(),
            b"42".as_slice(),
            b"null".as_slice(),
            b"true".as_slice(),
            b"[]".as_slice(),
            b"".as_slice(),
            // A byte-order mark is not JSON whitespace and `serde_json` says so.
            b"\xef\xbb\xbf{}".as_slice(),
        ] {
            object_from_slice::<Pair>(refused).expect_err("only an object is accepted");
        }
    }

    #[test]
    fn leading_whitespace_is_the_formats_business_not_this_modules() {
        let parsed: Pair =
            object_from_slice(b"  \n\t\r{\"provider\":\"a\",\"api_key\":\"b\"}").unwrap();

        assert_eq!(parsed.provider, "a");
    }

    #[test]
    fn trailing_bytes_are_refused_rather_than_half_read() {
        object_from_slice::<Pair>(br#"{"provider":"a","api_key":"b"} and then some"#)
            .expect_err("a body with trailing content is not one object");
    }

    #[test]
    fn a_borrowing_shape_reads_through_unchanged() {
        // The request bodies this guards are `#[serde(borrow)]`, so an adapter
        // that broke borrowing would be unusable at exactly the call sites that
        // need it most.
        let body = br#"{"host_id":"host-1"}"#;
        let parsed: Borrowed<'_> = object_from_slice(body).unwrap();

        assert_eq!(parsed.host_id, "host-1");
    }

    #[test]
    fn an_object_that_does_not_fit_still_fails_through_serde() {
        // The adapter constrains SHAPE and nothing else — a missing field is
        // serde's to report, with its own message, exactly as before.
        let failure = object_from_slice::<Pair>(br#"{"provider":"anthropic"}"#)
            .expect_err("a missing field is still a failure");

        assert!(
            failure.to_string().contains("api_key"),
            "serde's own diagnosis is preserved: {failure}"
        );
    }
}
