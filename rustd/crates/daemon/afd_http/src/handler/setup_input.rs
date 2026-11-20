//! Strict native mutation parsing keeps private input out of rejection logs.

use super::Refusal;
use af_core::id::Uuid7;
use http::{HeaderMap, header};
use serde::de::DeserializeOwned;

const INVALID: &str = "Setup input violates the supported shape or bounds";
const KEY: &str = "idempotency-key";
const KEY_REQUIRED: &str = "Send one Idempotency-Key containing a UUIDv7";

/// Room a request takes beside its text or Definition: the operation id, the
/// source tag, edit paths and the JSON framing around them.
const REQUEST_FIELDS_ALLOWANCE_BYTES: usize = 16 * 1024;
/// The largest body that carries one text.
///
/// The text at the ceiling, every character at its widest JSON escape, plus
/// the fields around it. A message typed into a running fleet, and a fleet's
/// message to another, are read up to this, so the transport never refuses
/// what the ceiling admits.
pub const TEXT_BODY_BYTES_MAX: usize =
    af_validate::TEXT_MAX_JSON_BYTES + REQUEST_FIELDS_ALLOWANCE_BYTES;
/// The largest setup request body: a draft created from a Definition at its
/// cap, with a first message at the text ceiling, so the envelope never
/// refuses what the Definition parser and the ceiling admit.
pub const SETUP_BODY_BYTES_MAX: usize =
    af_fleet_runtime::definition::MAX_DEFINITION_BYTES + TEXT_BODY_BYTES_MAX;

#[derive(garde::Validate)]
struct Envelope<'a> {
    #[garde(length(max = SETUP_BODY_BYTES_MAX))]
    bytes: &'a [u8],
}

/// Parses private JSON without logging authored key names.
///
/// # Errors
/// Returns a safe malformed-input refusal for invalid input.
pub fn body<T: DeserializeOwned>(body: &[u8]) -> Result<T, Refusal> {
    let envelope = garde::Unvalidated::new(Envelope { bytes: body })
        .validate()
        .map_err(|_invalid| {
            Refusal::coded(
                af_core::error_code::PAYLOAD_TOO_LARGE,
                "Setup request exceeds the supported envelope size",
            )
        })?;
    // Definition keys are private authored content. The generic HTTP helper
    // logs rejected key names, so use its underlying object parser directly.
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

/// Reads the required single `UUIDv7` replay header.
///
/// # Errors
/// Returns a safe malformed-input refusal for invalid input.
pub fn operation(headers: &HeaderMap) -> Result<Uuid7, Refusal> {
    let mut keys = headers.get_all(KEY).iter();
    let first = keys
        .next()
        .ok_or_else(|| Refusal::malformed(KEY_REQUIRED))?;
    if keys.next().is_some() {
        return Err(Refusal::malformed(KEY_REQUIRED));
    }
    first
        .to_str()
        .ok()
        .and_then(|value| Uuid7::parse(value).ok())
        .ok_or_else(|| Refusal::malformed(KEY_REQUIRED))
}

/// Parses a `UUIDv7` without exposing rejected input.
///
/// # Errors
/// Returns a safe malformed-input refusal for invalid input.
pub fn uuid(value: &str) -> Result<Uuid7, Refusal> {
    Uuid7::parse(value).map_err(|_invalid| Refusal::malformed(INVALID))
}

#[cfg(test)]
mod tests {
    use af_fleet_runtime::definition::MAX_DESCRIPTION_CHARS;
    use af_validate::TEXT_MAX_CHARS;
    use serde_json::{Value, json};

    use super::{SETUP_BODY_BYTES_MAX, TEXT_BODY_BYTES_MAX, body};

    /// The limits the app's setup and steer proxies read, shared as one file
    /// both sides test against (`ui/packages/app/tests/body-limits.test.ts`).
    const SHARED_LIMITS: &str = include_str!("../../tests/fixtures/body_limits.json");

    /// A create-draft body whose Definition fills both texts at their caps,
    /// with a first message at the text ceiling, all in the widest JSON
    /// encoding: every character a six-byte control escape.
    fn maximal_create_body() -> Vec<u8> {
        let widest = |chars: usize| "\u{1}".repeat(chars);
        let definition = json!({
            "schema_version": 1,
            "description": widest(MAX_DESCRIPTION_CHARS),
            "instructions_md": widest(TEXT_MAX_CHARS),
            "config": {"name": "maximal"},
        });
        let request = json!({
            "operation_id": "01920000-0000-7000-8000-000000000001",
            "source": {"kind": "definition", "definition": definition},
            "message": widest(TEXT_MAX_CHARS),
        });
        serde_json::to_vec(&request).unwrap_or_default()
    }

    #[test]
    fn a_maximal_valid_definition_fits_the_setup_envelope() {
        let bytes = maximal_create_body();
        assert!(
            bytes.len() > af_wire::run::SETUP_DEFINITION_BYTES_MAX,
            "the old cap refused this"
        );
        assert!(bytes.len() <= SETUP_BODY_BYTES_MAX, "{} bytes", bytes.len());
        let parsed = body::<Value>(&bytes).map(|document| document.is_object());
        assert_eq!(
            parsed.ok(),
            Some(true),
            "the maximal request parses as an object"
        );
    }

    /// A steer body whose message sits at the ceiling in the widest JSON
    /// encoding, with an operation id at its own bound, fits the text body.
    #[test]
    fn a_widest_message_at_the_ceiling_fits_the_text_body() {
        let request = json!({
            "message": "\u{1}".repeat(TEXT_MAX_CHARS),
            "operation_id": "\u{1}".repeat(200),
        });
        let bytes = serde_json::to_vec(&request).unwrap_or_default();
        assert!(bytes.len() > 6 * 1024 * 1024, "{} bytes", bytes.len());
        assert!(bytes.len() <= TEXT_BODY_BYTES_MAX, "{} bytes", bytes.len());
    }

    /// The daemon's two body limits are the ones the app's proxies read: the
    /// shared file states them, and both sides assert against it.
    #[test]
    fn the_body_limits_are_the_ones_the_app_proxies_read() {
        let shared: Value = serde_json::from_str(SHARED_LIMITS).unwrap_or_default();
        assert_eq!(
            shared.get("setup_body_bytes_max").and_then(Value::as_u64),
            u64::try_from(SETUP_BODY_BYTES_MAX).ok()
        );
        assert_eq!(
            shared.get("text_body_bytes_max").and_then(Value::as_u64),
            u64::try_from(TEXT_BODY_BYTES_MAX).ok()
        );
        assert_eq!(
            (SETUP_BODY_BYTES_MAX, TEXT_BODY_BYTES_MAX),
            (12_861_440, 6_307_840)
        );
    }

    #[test]
    fn a_body_past_the_setup_envelope_is_refused_as_too_large() {
        let filler = " ".repeat(SETUP_BODY_BYTES_MAX);
        let bytes = format!("{{\"pad\":\"{filler}\"}}").into_bytes();
        let refused = body::<Value>(&bytes).err();
        assert_eq!(
            refused.map(|refusal| refusal.status()),
            Some(http::StatusCode::PAYLOAD_TOO_LARGE)
        );
    }
}
