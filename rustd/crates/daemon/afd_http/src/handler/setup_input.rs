//! Strict fleet mutation parsing keeps a person's input out of rejection logs.

use super::Refusal;
use http::{HeaderMap, header};
use serde::de::DeserializeOwned;

const INVALID: &str = "Input violates the supported shape or bounds";

/// Room a request takes beside its text: the operation id, the model and name
/// fields, and the JSON framing around them.
const REQUEST_FIELDS_ALLOWANCE_BYTES: usize = 16 * 1024;
/// The largest body that carries one text.
///
/// The text at the ceiling, every character at the widest JSON escape a
/// client may send (an astral character as a twelve-byte surrogate pair),
/// plus the fields around it. A fleet's first message, a message typed into a
/// running fleet, and a fleet's message to another are read up to this, so
/// the transport never refuses what the ceiling admits, and a text past the
/// ceiling earns the ceiling's own refusal.
pub const TEXT_BODY_BYTES_MAX: usize =
    af_validate::TEXT_MAX_JSON_BYTES + REQUEST_FIELDS_ALLOWANCE_BYTES;
#[derive(garde::Validate)]
struct Envelope<'a> {
    #[garde(length(max = TEXT_BODY_BYTES_MAX))]
    bytes: &'a [u8],
}

/// Parses a person's JSON without logging the key names they sent.
///
/// # Errors
/// Returns a safe malformed-input refusal for invalid input.
pub fn body<T: DeserializeOwned>(body: &[u8]) -> Result<T, Refusal> {
    let envelope = garde::Unvalidated::new(Envelope { bytes: body })
        .validate()
        .map_err(|_invalid| {
            Refusal::coded(
                af_core::error_code::PAYLOAD_TOO_LARGE,
                "Request exceeds the supported envelope size",
            )
        })?;
    // The generic HTTP helper logs rejected key names, which are a person's
    // input here, so use its underlying object parser directly.
    af_core::json::object_from_slice(envelope.bytes).map_err(|_invalid| Refusal::malformed(INVALID))
}

/// Reads exactly one conditional resource tag.
///
/// # Errors
/// Returns a safe malformed-input refusal for invalid input.
pub fn expected(headers: &HeaderMap) -> Result<Option<&str>, Refusal> {
    let mut tags = headers.get_all(header::IF_MATCH).iter();
    let Some(tag) = tags.next() else {
        return Ok(None);
    };
    if tags.next().is_some() {
        return Err(Refusal::malformed("Send one strong If-Match tag"));
    }
    tag.to_str()
        .map(Some)
        .map_err(|_invalid| Refusal::malformed(INVALID))
}

#[cfg(test)]
mod tests {
    use af_validate::TEXT_MAX_CHARS;
    use serde_json::{Value, json};

    use super::{TEXT_BODY_BYTES_MAX, body};

    /// The limits the app's proxies read, shared as one file both sides test
    /// against (`ui/packages/app/tests/body-limits.test.ts`).
    const SHARED_LIMITS: &str = include_str!("../../tests/fixtures/body_limits.json");

    /// A steer body whose message sits at the ceiling in the widest JSON
    /// encoding, with an operation id at its own bound, fits the text body.
    #[test]
    fn a_widest_message_at_the_ceiling_fits_the_text_body() {
        let request = json!({
            "message": "\u{1}".repeat(TEXT_MAX_CHARS),
            "operation_id": "\u{1}".repeat(af_wire::turn::OPERATION_ID_MAX_BYTES),
        });
        let bytes = serde_json::to_vec(&request).unwrap_or_default();
        assert!(bytes.len() > 6 * 1024 * 1024, "{} bytes", bytes.len());
        assert!(bytes.len() <= TEXT_BODY_BYTES_MAX, "{} bytes", bytes.len());
    }

    /// The daemon's text body limit is the one the app's proxies read: the
    /// shared file states it, and both sides assert against it.
    #[test]
    fn the_text_body_limit_is_the_one_the_app_proxies_read() {
        let shared: Value = serde_json::from_str(SHARED_LIMITS).unwrap_or_default();
        assert_eq!(
            shared.get("text_body_bytes_max").and_then(Value::as_u64),
            u64::try_from(TEXT_BODY_BYTES_MAX).ok()
        );
        assert_eq!(TEXT_BODY_BYTES_MAX, 12_599_296);
    }

    /// `count` astral characters, each as the twelve-byte surrogate-pair
    /// escape a client may send, inside a JSON string's quotes.
    fn astral(count: usize) -> String {
        format!("\"{}\"", r"\uD83D\uDE00".repeat(count))
    }

    /// A message one character past the ceiling, every character an escaped
    /// astral one, still fits the text body, so the route answers it with the
    /// ceiling's refusal rather than the transport's: the message's own bound
    /// is what refuses it.
    #[test]
    fn a_message_past_the_ceiling_at_a_clients_widest_escape_meets_the_ceiling()
    -> Result<(), Box<dyn std::error::Error>> {
        let operation = "\u{1}".repeat(200);
        let sent = format!(
            r#"{{"message":{},"operation_id":{}}}"#,
            astral(TEXT_MAX_CHARS + 1),
            serde_json::to_string(&operation).unwrap_or_default(),
        );
        assert!(sent.len() <= TEXT_BODY_BYTES_MAX, "{} bytes", sent.len());
        let request: af_wire::turn::SteerRequest<'_> = serde_json::from_str(&sent)?;
        let refused = garde::Validate::validate(&request).err();
        assert!(
            refused
                .is_some_and(|report| report.iter().any(|(path, _)| path.to_string() == "message")),
            "the message's own bound refuses it"
        );
        let at_ceiling = format!(r#"{{"message":{}}}"#, astral(TEXT_MAX_CHARS));
        let request: af_wire::turn::SteerRequest<'_> = serde_json::from_str(&at_ceiling)?;
        garde::Validate::validate(&request)?;
        Ok(())
    }

    /// A fleet's first message at the ceiling, every character an escaped
    /// astral one, beside its operation id, fits the envelope.
    #[test]
    fn a_first_message_at_a_clients_widest_escape_fits_the_envelope() {
        let sent = format!(
            r#"{{"operation_id":"01920000-0000-7000-8000-000000000001","message":{}}}"#,
            astral(TEXT_MAX_CHARS),
        );
        assert!(sent.len() > 12 * 1024 * 1024, "{} bytes", sent.len());
        let parsed = body::<Value>(sent.as_bytes()).map(|document| document.is_object());
        assert_eq!(parsed.ok(), Some(true), "{} bytes", sent.len());
    }

    #[test]
    fn a_body_past_the_envelope_is_refused_as_too_large() {
        let filler = " ".repeat(TEXT_BODY_BYTES_MAX);
        let bytes = format!("{{\"pad\":\"{filler}\"}}").into_bytes();
        let refused = body::<Value>(&bytes).err();
        assert_eq!(
            refused.map(|refusal| refusal.status()),
            Some(http::StatusCode::PAYLOAD_TOO_LARGE)
        );
    }

    /// The refusal as a caller reads it: status, registry code and sentence.
    async fn problem(refusal: super::Refusal) -> (http::StatusCode, Value) {
        let response = axum::response::IntoResponse::into_response(refusal);
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap_or_default();
        (status, serde_json::from_slice(&bytes).unwrap_or_default())
    }

    fn headers(name: &'static str, values: &[&[u8]]) -> http::HeaderMap {
        let mut headers = http::HeaderMap::new();
        for value in values {
            if let Ok(value) = http::HeaderValue::from_bytes(value) {
                headers.append(name, value);
            }
        }
        headers
    }

    #[tokio::test]
    async fn a_repeated_or_unreadable_if_match_is_refused_and_one_tag_is_read()
    -> Result<(), Box<dyn std::error::Error>> {
        for (values, detail) in [
            (
                &[b"\"a\"".as_slice(), b"\"b\""][..],
                "Send one strong If-Match tag",
            ),
            (&[b"\"caf\xc3\xa9\"".as_slice()][..], super::INVALID),
        ] {
            let refused = super::expected(&headers("if-match", values)).err();
            let (status, body) = problem(refused.ok_or("the tag was accepted")?).await;
            assert_eq!(status, http::StatusCode::BAD_REQUEST);
            assert_eq!(
                body.get("error_code").and_then(Value::as_str),
                Some("UZ-REQ-001")
            );
            assert_eq!(body.get("detail").and_then(Value::as_str), Some(detail));
        }
        let one = headers("if-match", &[b"\"a\""]);
        assert_eq!(super::expected(&one).ok(), Some(Some("\"a\"")));
        assert_eq!(super::expected(&http::HeaderMap::new()).ok(), Some(None));
        Ok(())
    }
}
