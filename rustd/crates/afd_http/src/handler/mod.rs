//! Shared request parsing and refusal rendering for every API plane.

mod refusable;
mod refusal;

use std::borrow::Cow;

use axum::response::{IntoResponse as _, Response};

pub use self::refusable::{Refusable, refuse};
pub use self::refusal::Refusal;

/// Refuses a request this daemon cannot read at all.
#[must_use]
pub fn malformed(detail: &'static str) -> Response {
    reject(afd_core::error_code::INVALID_REQUEST, detail)
}

/// Writes a registry refusal whose detail only the call site knows.
#[must_use]
pub fn reject(code: afd_core::error_code::ErrorCode, detail: &'static str) -> Response {
    crate::envelope::ProblemResponse::new(code, detail, crate::request_id::RequestId::mint())
        .into_response()
}

/// Returns one raw query-string parameter by name.
#[must_use]
pub fn parameter<'q>(query: &'q str, name: &str) -> Option<&'q str> {
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == name).then_some(value)
    })
}

/// A broken percent escape or a decoded value that is not UTF-8.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrokenEscape;

/// Returns one query parameter with URL percent escapes decoded.
///
/// # Errors
/// Returns [`BrokenEscape`] for incomplete or non-hex escapes and for decoded
/// bytes that are not UTF-8.
pub fn decoded_parameter<'q>(
    query: &'q str,
    name: &str,
) -> Result<Option<Cow<'q, str>>, BrokenEscape> {
    let Some(raw) = parameter(query, name) else {
        return Ok(None);
    };
    if !raw.bytes().any(|byte| byte == b'%' || byte == b'+') {
        return Ok(Some(Cow::Borrowed(raw)));
    }
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while let Some(&byte) = bytes.get(index) {
        match byte {
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            b'%' => {
                let high = bytes.get(index + 1).copied().and_then(hex_value);
                let low = bytes.get(index + 2).copied().and_then(hex_value);
                let (Some(high), Some(low)) = (high, low) else {
                    return Err(BrokenEscape);
                };
                decoded.push(high << 4 | low);
                index += 3;
            }
            other => {
                decoded.push(other);
                index += 1;
            }
        }
    }
    String::from_utf8(decoded)
        .map(Cow::Owned)
        .map(Some)
        .map_err(|_not_text| BrokenEscape)
}

const fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
