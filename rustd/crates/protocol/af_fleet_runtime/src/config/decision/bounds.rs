//! Complete authored questions are bounded before becoming immutable setup.

use std::collections::BTreeMap;

use af_validate::TEXT_MAX_CHARS;
use garde::Validate;

use super::raw;
use crate::Result;

#[derive(Validate)]
struct Text<'a> {
    #[garde(
        length(chars, min = 1, max = TEXT_MAX_CHARS),
        custom(nonblank),
        custom(af_validate::rules::nul_free)
    )]
    value: &'a str,
}

#[derive(Validate)]
struct Statements<'a> {
    #[garde(length(min = 1))]
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

pub(super) const fn statement_char(value: char) -> bool {
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

pub(super) fn statements(values: &[raw::Statement]) -> Result<()> {
    garde::Unvalidated::new(Statements { values }).validate()?;
    Ok(())
}
