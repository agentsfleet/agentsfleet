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
    let value = T::deserialize(ObjectOnly(&mut format))?;
    // What `from_slice` does after its own parse: refuse trailing bytes, so
    // `{} garbage` is not silently half-read.
    format.end()?;
    Ok(value)
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
