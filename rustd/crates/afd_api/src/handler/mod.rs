//! The verbs this binary answers, and the one way they refuse.
//!
//! A handler here does three things and no others: read what the request
//! carries, call one service method, and turn the answer into a response. It
//! decides nothing — the identity was proven by the layer in front of it, the
//! validation lives in the service, and the status a refusal answers with is a
//! property of the error's code rather than of the call site.
//!
//! # Why the refusal writer is shared
//!
//! `service.zig`'s handlers each spell their own `hx.fail(code, detail)` pairs,
//! twelve times over, and nothing relates a code to its sentence — so two
//! handlers can describe one failure differently and both compile. Here
//! the shared refusal writer takes the ERROR, and the error already knows both
//! (`afd_fleet::Error::code` and `::detail`). There is no pair to get wrong
//! because there is no pair to write. What each plane can be asked about its
//! own failure is the `Refusable` trait, in its own file: that list grows
//! once per crate with a fallible surface, and this one grows once per verb.

pub mod approval;
pub mod auth;
pub mod event;
pub mod fleet;
pub mod grant;
pub mod preference;
pub mod runner;
pub mod secret;
pub mod stream;
pub mod tenant;

mod refusable;
mod refusal;

use std::borrow::Cow;

use axum::response::{IntoResponse as _, Response};

pub(crate) use self::refusable::refuse;
pub(crate) use self::refusal::Refusal;

/// Refuses a request this daemon cannot read at all.
///
/// A path segment that is not an identifier, or a body that is not the shape
/// the verb takes, can never reach a row — so it is refused BEFORE the plane is
/// asked. That keeps the `::uuid` cast in the statements from ever being the
/// thing that fails, and leaves every error from below a genuine datastore
/// fault.
///
/// Shared by every verb that reads one, rather than restated per handler: two
/// spellings would be two different envelopes for one class of refusal.
pub(crate) fn malformed(detail: &'static str) -> Response {
    crate::envelope::ProblemResponse::new(
        afd_core::error_code::INVALID_REQUEST,
        detail,
        crate::request_id::RequestId::mint(),
    )
    .into_response()
}

/// One query-string parameter, by name.
///
/// A hand-rolled scan rather than a query-string crate, because that is the
/// whole of what these handlers need from a query string and a crate for it
/// would be a dependency to justify. Percent-decoding is deliberately absent:
/// every value these parameters take — a limit, a sort spelling, a cursor —
/// is drawn from an alphabet that survives a URL unescaped, and a decoder here
/// would be a second place for a `+` to become a space.
pub(crate) fn parameter<'q>(query: &'q str, name: &str) -> Option<&'q str> {
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == name).then_some(value)
    })
}

/// A broken percent-escape, or bytes that decode to no UTF-8 — the caller
/// owns the sentence, because two route families refuse it differently.
pub(crate) struct BrokenEscape;

/// One query parameter, percent-decoded the way httpz's `unescape` does.
///
/// [`parameter`] scans RAW values because some alphabets here survive a URL
/// unescaped — a cursor, a limit, a `UUIDv7`. Everything a person can type does
/// not: a workspace name, a provider filter, an actor glob, a timestamp with
/// its colons. Those decode through this — `%XX` bytes, `+` as space, and a
/// stray or short escape refusing the value.
///
/// This IS what the daemon it ports does. `httpz`'s `req.query()` calls
/// `Url.unescape` on every value, so a route reading raw bytes is not
/// declining to decode — it is disagreeing with the oracle.
pub(crate) fn decoded_parameter<'q>(
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
    let text = String::from_utf8(decoded).map_err(|_not_text| BrokenEscape)?;
    Ok(Some(Cow::Owned(text)))
}

/// One hex digit's value, either case.
const fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
