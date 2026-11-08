//! Strict URL escape validation around the standard percent decoder.
use std::borrow::Cow;

/// A broken percent escape or decoded text that is not UTF-8.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrokenEscape;

fn validate(raw: &str) -> Result<(), BrokenEscape> {
    for escaped in raw.as_bytes().split(|byte| *byte == b'%').skip(1) {
        if !escaped
            .get(..2)
            .is_some_and(|digits| digits.iter().all(u8::is_ascii_hexdigit))
        {
            return Err(BrokenEscape);
        }
    }
    Ok(())
}

/// Decodes a path value, preserving literal `+` and arbitrary decoded bytes.
///
/// # Errors
/// Refuses incomplete or non-hex percent escapes.
pub fn decode_bytes(raw: &str) -> Result<Vec<u8>, BrokenEscape> {
    validate(raw)?;
    Ok(percent_encoding::percent_decode_str(raw).collect())
}

/// Decodes one form name or value, treating literal `+` as a space.
///
/// # Errors
/// Refuses malformed percent escapes and invalid UTF-8.
pub fn decode_form(raw: &str) -> Result<Cow<'_, str>, BrokenEscape> {
    validate(raw)?;
    if raw.contains('+') {
        percent_encoding::percent_decode_str(&raw.replace('+', " "))
            .decode_utf8()
            .map(|decoded| Cow::Owned(decoded.into_owned()))
            .map_err(|_invalid| BrokenEscape)
    } else {
        percent_encoding::percent_decode_str(raw)
            .decode_utf8()
            .map_err(|_invalid| BrokenEscape)
    }
}
