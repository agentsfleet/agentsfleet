//! Strict native mutation parsing keeps private input out of rejection logs.

use super::Refusal;
use afd_core::id::Uuid7;
use http::{HeaderMap, header};
use serde::de::DeserializeOwned;

const INVALID: &str = "Setup input violates the supported shape or bounds";
const KEY: &str = "idempotency-key";
const KEY_REQUIRED: &str = "Send one Idempotency-Key containing a UUIDv7";

#[derive(garde::Validate)]
struct Envelope<'a> {
    #[garde(length(max = afd_wire::authoring::ENVELOPE_BYTES_MAX))]
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
