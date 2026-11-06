//! Complete authored questions are bounded before becoming immutable setup.

use std::collections::BTreeMap;

use garde::Validate;
use serde::Serialize;

use super::{MAX_COMPLETION_STATEMENTS, MAX_QUESTION_TEXT_BYTES, raw};
use crate::Result;

/// Authored representation; the rendered provider request is proved separately.
const MAX_AUTHORED_QUESTION_BYTES: usize = 1024;

#[derive(Validate)]
struct Text<'a> {
    #[garde(
        length(bytes, min = 1, max = MAX_QUESTION_TEXT_BYTES),
        custom(nonblank),
        custom(afd_validate::rules::nul_free)
    )]
    value: &'a str,
}

#[derive(Validate)]
struct Encoded<'a, T: Serialize> {
    #[garde(custom(encoded))]
    value: &'a T,
}

#[derive(Validate)]
struct Statements<'a> {
    #[garde(length(min = 1, max = MAX_COMPLETION_STATEMENTS))]
    values: &'a [raw::Statement],
}

pub(super) fn nonblank<C: ?Sized>(value: &str, _context: &C) -> garde::Result {
    if value.trim().is_empty() {
        Err(garde::Error::new(
            "question text and criteria must not be blank",
        ))
    } else {
        Ok(())
    }
}

pub(super) fn statement_char(value: char) -> bool {
    value.is_ascii_alphanumeric() || matches!(value, '_' | '.' | '-')
}

pub(super) fn criteria<C: ?Sized>(
    values: &BTreeMap<String, String>,
    _context: &C,
) -> garde::Result {
    for value in values.values() {
        if (Text { value }).validate().is_err() {
            return Err(garde::Error::new(
                "each criterion must be bounded, nonblank and NUL-free",
            ));
        }
    }
    Ok(())
}

fn encoded<T: Serialize, C: ?Sized>(value: &T, _context: &C) -> garde::Result {
    if afd_validate::serialized_within(value, MAX_AUTHORED_QUESTION_BYTES) {
        Ok(())
    } else {
        Err(garde::Error::new(
            "complete authored question exceeds 1 KiB",
        ))
    }
}

pub(super) fn question<T: Serialize>(value: &T) -> Result<()> {
    garde::Unvalidated::new(Encoded { value }).validate()?;
    Ok(())
}

pub(super) fn statements(values: &[raw::Statement]) -> Result<()> {
    garde::Unvalidated::new(Statements { values }).validate()?;
    Ok(())
}
