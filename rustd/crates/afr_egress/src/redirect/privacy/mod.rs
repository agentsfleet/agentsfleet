//! Only finite, known download-target spellings are masked.

use reqwest::Url;

use super::{Stop, Target};
use crate::{Inbound, RESPONSE_MAX_BYTES, Result};

mod matcher;
use matcher::Matcher;

const MASK: &str = "«secret:redirect»";
const CREDENTIAL_KEYS: [&str; 11] = [
    "sig",
    "signature",
    "token",
    "secret",
    "key",
    "credential",
    "password",
    "auth",
    "authorization",
    "code",
    "session",
];

#[derive(Default)]
pub(super) struct Privacy {
    units: Vec<String>,
}

impl Privacy {
    pub(super) fn add(&mut self, target: &Target) {
        // Pure navigation carries no private path; its resolved URL stays protected.
        if target
            .raw
            .chars()
            .any(|c| !matches!(c, '/' | '.' | '?' | '#'))
        {
            self.units.push(target.raw.clone());
        }
        self.units.push(target.resolved.to_string());
        let path = &target.resolved[url::Position::BeforePath..url::Position::AfterQuery];
        if path != "/" && !path.is_empty() {
            self.units.push(path.to_owned());
        }
        self.query_values(&target.resolved);
    }

    fn query_values(&mut self, url: &Url) {
        for (written, (name, value)) in
            url.query()
                .unwrap_or_default()
                .split('&')
                .filter_map(|written| {
                    url::form_urlencoded::parse(written.as_bytes())
                        .next()
                        .map(|pair| (written, pair))
                })
        {
            let credential = name
                .split(|c: char| !c.is_ascii_alphanumeric())
                .any(|token| {
                    CREDENTIAL_KEYS
                        .iter()
                        .any(|key| token.eq_ignore_ascii_case(key))
                });
            // The parser decides the field; retain its original value spelling for masking.
            let raw = written.split_once('=').map_or("", |(_, value)| value);
            if credential || raw.len() >= 16 || value.len() >= 16 {
                self.units.extend([raw.to_owned(), value.into_owned()]);
            }
        }
    }

    pub(super) fn clean(&self, response: &mut Inbound) -> Result<(), Stop> {
        if self.units.is_empty() {
            return Ok(());
        }
        let mut values = Vec::new();
        for unit in self.units.iter().filter(|unit| !unit.is_empty()) {
            values.extend(spellings(unit)?);
        }
        values.sort_unstable();
        values.dedup();
        let matcher = Matcher::new(&values)?;
        let (body, truncated) = mask_bounded(&matcher, &response.body)?;
        response.body = body;
        response.truncated |= truncated;
        if let Some(content_type) = &mut response.content_type {
            let (masked, truncated) = mask_bounded(&matcher, content_type)?;
            if truncated {
                return Err(Stop::Privacy);
            }
            *content_type = masked;
        }
        Ok(())
    }
}

/// Keep redactions whole and never build an expanded response beyond its budget.
fn mask_bounded(matcher: &Matcher, text: &str) -> Result<(String, bool), Stop> {
    let mut result = String::with_capacity(text.len().min(RESPONSE_MAX_BYTES));
    let mut cursor = 0;
    for covered in matcher.covered(text)? {
        let plain = text.get(cursor..covered.start).ok_or(Stop::Privacy)?;
        if !append_bounded(&mut result, plain) || RESPONSE_MAX_BYTES - result.len() < MASK.len() {
            return Ok((result, true));
        }
        result.push_str(MASK);
        cursor = covered.end;
    }
    let tail = text.get(cursor..).ok_or(Stop::Privacy)?;
    let complete = append_bounded(&mut result, tail);
    Ok((result, !complete))
}

fn append_bounded(result: &mut String, text: &str) -> bool {
    let room = RESPONSE_MAX_BYTES - result.len();
    let end = text.floor_char_boundary(room.min(text.len()));
    result.push_str(text.get(..end).unwrap_or_default());
    end == text.len()
}

fn spellings(value: &str) -> Result<[String; 5], Stop> {
    let encoded: String = url::form_urlencoded::byte_serialize(value.as_bytes()).collect();
    // Form encoding and URI-component encoding differ for space, '*' and '~'.
    let component = encoded
        .replace('+', "%20")
        .replace('*', "%2A")
        .replace("%7E", "~");
    let json = serde_json::to_string(value).map_err(|_invalid| Stop::Privacy)?;
    let escaped = json
        .strip_prefix('"')
        .and_then(|text| text.strip_suffix('"'))
        .ok_or(Stop::Privacy)?
        .to_owned();
    let slashes = escaped.replace('/', "\\/");
    Ok([value.to_owned(), encoded, component, escaped, slashes])
}

/// Percent hex digits are case-insensitive; the remaining signature bytes are not.
fn encoded_pattern(value: &str) -> String {
    let mut result = String::new();
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        result.push_str(&regex::escape(&c.to_string()));
        if c == '%' {
            let digits = hex_digits(&mut chars, &mut result, 2);
            if digits == "25" {
                hex_digits(&mut chars, &mut result, 2);
            }
        } else if c == '\\' && chars.peek() == Some(&'u') {
            chars.next();
            result.push('u');
            hex_digits(&mut chars, &mut result, 4);
        }
    }
    result
}

fn hex_digits(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    result: &mut String,
    count: usize,
) -> String {
    let mut digits = String::new();
    for _digit in 0..count {
        let Some(hex) = chars.next_if(char::is_ascii_hexdigit) else {
            break;
        };
        digits.push(hex);
        if hex.is_ascii_alphabetic() {
            result.extend(['[', hex.to_ascii_lowercase(), hex.to_ascii_uppercase(), ']']);
        } else {
            result.push(hex);
        }
    }
    digits
}
