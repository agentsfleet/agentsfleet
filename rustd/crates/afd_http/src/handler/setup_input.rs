//! Strict native mutation parsing keeps private input out of rejection logs.

use super::Refusal;
use afd_core::id::Uuid7;
use http::{HeaderMap, header};
use serde::de::DeserializeOwned;

const INVALID: &str = "Setup input violates the supported shape or bounds";
const KEY: &str = "idempotency-key";
const KEY_REQUIRED: &str = "Send one Idempotency-Key containing a UUIDv7";

/// Room a setup request takes beside its Definition: the operation id, the
/// source tag, edit paths and the JSON framing around them.
const REQUEST_FIELDS_ALLOWANCE_BYTES: usize = 16 * 1024;
/// The largest setup request body: a Definition at its cap plus the fields
/// around it, so the envelope never refuses what the Definition parser admits.
pub const SETUP_BODY_BYTES_MAX: usize =
    afd_fleet_runtime::definition::MAX_DEFINITION_BYTES + REQUEST_FIELDS_ALLOWANCE_BYTES;

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
                afd_core::error_code::PAYLOAD_TOO_LARGE,
                "Setup request exceeds the supported envelope size",
            )
        })?;
    // Definition keys are private authored content. The generic HTTP helper
    // logs rejected key names, so use its underlying object parser directly.
    afd_core::json::object_from_slice(envelope.bytes)
        .map_err(|_invalid| Refusal::malformed(INVALID))
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
    use afd_fleet_runtime::definition::{MAX_DESCRIPTION_CHARS, MAX_INSTRUCTIONS_CHARS};
    use serde_json::{Value, json};

    use super::{SETUP_BODY_BYTES_MAX, body};

    /// A create-draft body whose Definition fills both texts at their caps in
    /// the widest JSON encoding: every character a six-byte control escape.
    fn maximal_create_body() -> Vec<u8> {
        let widest = |chars: usize| "\u{1}".repeat(chars);
        let definition = json!({
            "schema_version": 1,
            "description": widest(MAX_DESCRIPTION_CHARS),
            "instructions_md": widest(MAX_INSTRUCTIONS_CHARS),
            "config": {"name": "maximal"},
        });
        let request = json!({
            "operation_id": "01920000-0000-7000-8000-000000000001",
            "source": {"kind": "definition", "definition": definition},
        });
        serde_json::to_vec(&request).unwrap_or_default()
    }

    #[test]
    fn a_maximal_valid_definition_fits_the_setup_envelope() {
        let bytes = maximal_create_body();
        assert!(
            bytes.len() > afd_wire::authoring::ENVELOPE_BYTES_MAX,
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
