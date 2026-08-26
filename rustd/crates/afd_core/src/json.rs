//! Reading a JSON OBJECT into a type, and refusing anything that is not one.
//!
//! # The hole this closes
//!
//! `serde_json` fills a derived struct from a JSON ARRAY, taking its elements
//! POSITIONALLY. `["anthropic","sk-live"]` deserializes into a two-field
//! credential as happily as `{"provider":…,"api_key":…}` does, and every shape
//! check downstream then passes, because the value really does have both
//! fields. That is by design — serde's derive implements `visit_seq` alongside
//! `visit_map` so the same type can ride a self-describing format and a compact
//! one — and it is a hole wherever the JSON came from outside this process,
//! because every contract this daemon publishes says object.
//!
//! `#[serde(deny_unknown_fields)]` does NOT close it: an array has no field
//! names to be unknown.
//!
//! # Why a leading-byte gate and not a second parse
//!
//! The obvious pure-serde route is to deserialize into
//! [`serde_json::Map`](serde_json::Map) or a [`Value`](serde_json::Value)
//! first, which genuinely does reject an array, and then convert. Two things
//! rule it out:
//!
//! - It parses the document TWICE, on paths that run per request.
//! - The intermediate holds every string in the document in a tree with no
//!   destructor. One caller is a vault credential, and its whole point is that
//!   the key lives in something that wipes itself — an interposed `Value` is
//!   exactly the un-wipeable copy that type exists to prevent.
//!
//! A per-type hand-written `Deserialize` that implements only `visit_map` is
//! airtight and gives up the derive, at roughly twenty-five lines per type.
//!
//! So the gate is structural and O(1): the first non-whitespace byte of a JSON
//! object is `{`, and of nothing else is. If it is `{`, the document is either
//! a valid object or invalid JSON, and `serde_json` decides which — this
//! function does not parse anything. Anything else, BOM and all, is refused
//! here without allocating. It fails closed in both directions.

use serde::Deserialize;
use serde::de::Error as _;

/// What a body that is not a JSON object is reported as.
const NOT_AN_OBJECT: &str = "expected a JSON object";

/// Deserializes `body` into `T`, refusing any JSON that is not an object.
///
/// A drop-in for [`serde_json::from_slice`] at a trust boundary: same signature,
/// same error type, so a call site keeps whatever it already does with the
/// failure. Borrowing types are supported — `body` outlives the result — so a
/// `#[serde(borrow)]` request shape reads through this unchanged.
///
/// # Errors
/// Returns a `serde_json` error when the body is not a JSON object, and
/// whatever `serde_json` itself reports when it is one that does not fit `T`.
pub fn object_from_slice<'de, T>(body: &'de [u8]) -> Result<T, serde_json::Error>
where
    T: Deserialize<'de>,
{
    if !opens_an_object(body) {
        return Err(serde_json::Error::custom(NOT_AN_OBJECT));
    }
    serde_json::from_slice(body)
}

/// Whether `body`'s first non-whitespace byte opens an object.
///
/// `is_ascii_whitespace` is WIDER than JSON's own whitespace set — it admits a
/// form feed, which JSON does not. That is deliberate and safe in this
/// direction: a body this skips past still has to satisfy `serde_json`, which
/// rejects the form feed itself. Being stricter here would refuse a document
/// twice for the same reason and put the refusal in the less specific place.
fn opens_an_object(body: &[u8]) -> bool {
    body.iter()
        .find(|byte| !byte.is_ascii_whitespace())
        .is_some_and(|byte| *byte == b'{')
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

        object_from_slice::<Pair>(br#"["anthropic","sk-live"]"#)
            .expect_err("an array is not an object, however well it lines up");
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
            // A byte-order mark is not whitespace and does not open an object.
            b"\xef\xbb\xbf{}".as_slice(),
        ] {
            object_from_slice::<Pair>(refused).expect_err("only an object is accepted");
        }
    }

    #[test]
    fn leading_whitespace_is_skipped_before_the_shape_is_judged() {
        let parsed: Pair =
            object_from_slice(b"  \n\t\r{\"provider\":\"a\",\"api_key\":\"b\"}").unwrap();

        assert_eq!(parsed.provider, "a");
    }

    #[test]
    fn a_borrowing_shape_reads_through_unchanged() {
        // The request bodies this guards are `#[serde(borrow)]`, so a helper
        // bounded on `DeserializeOwned` would have been unusable at exactly the
        // call sites that need it most.
        let body = br#"{"host_id":"host-1"}"#;
        let parsed: Borrowed<'_> = object_from_slice(body).unwrap();

        assert_eq!(parsed.host_id, "host-1");
    }

    #[test]
    fn an_object_that_does_not_fit_still_fails_through_serde() {
        // The gate judges SHAPE and nothing else — a missing field is
        // `serde_json`'s to report, with its own message, exactly as before.
        let failure = object_from_slice::<Pair>(br#"{"provider":"anthropic"}"#)
            .expect_err("a missing field is still a failure");

        assert!(
            failure.to_string().contains("api_key"),
            "serde's own diagnosis is preserved: {failure}"
        );
    }
}
