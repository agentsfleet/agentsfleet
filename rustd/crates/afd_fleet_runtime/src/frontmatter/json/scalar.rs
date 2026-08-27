//! `writeScalar`'s coercion table, which is the whole type system a fleet
//! document has.
//!
//! # Stricter than YAML, and that is the point
//!
//! Exactly `true`/`false` are booleans, exactly `null`/`~` are null, a scalar
//! passing `is_numeric` is written through as a bare JSON number, and every
//! other scalar — including `True`, `yes`, `on`, `1e5`, `01`, `+1`, `0x1F`,
//! `NaN` — is a JSON string.
//!
//! This table is why the tokeniser next door resolves nothing. A YAML crate
//! that types scalars for us types them WRONG here and unrecoverably: `01`
//! becomes the integer 1 and `1e5` becomes the float 100000, where this
//! product's answer is the strings `"01"` and `"1e5"`.
//!
//! Quote style reaches this module and is deliberately not consulted — see
//! divergence 1 in the parent module.

use serde_json::Value;

/// The one scalar spelling that is boolean true.
const TRUE: &str = "true";

/// The one scalar spelling that is boolean false.
const FALSE: &str = "false";

/// The long spelling of an absent value.
const NULL: &str = "null";

/// The short spelling of an absent value.
///
/// `saphyr-parser` also normalises an EMPTY value (`key:` with nothing after
/// it) to this, which is the same answer the Zig reaches by a different route:
/// its parser yields `Value.empty` and `writeJsonValue` writes `null`.
const TILDE: &str = "~";

/// One scalar's JSON value, by `writeScalar`'s table.
///
/// The quote style is deliberately not consulted — see this module's
/// divergence 1.
pub(super) fn scalar_value(raw: &str) -> Value {
    match raw {
        TRUE => Value::Bool(true),
        FALSE => Value::Bool(false),
        NULL | TILDE => Value::Null,
        _ if is_numeric(raw) => numeric_value(raw),
        _ => Value::String(raw.to_owned()),
    }
}

/// A numeric scalar, read as the number its authored bytes spell.
///
/// [`is_numeric`] admits only the JSON number grammar minus exponents, so
/// serde reads every string that reaches here. The fallback keeps the function
/// total rather than resting that argument on a runtime panic.
fn numeric_value(raw: &str) -> Value {
    serde_json::from_str(raw).unwrap_or_else(|_unreadable| Value::String(raw.to_owned()))
}

/// Whether a scalar is written through as a bare JSON number.
///
/// A direct port of `yaml_frontmatter.zig`'s `isNumeric`, and stricter than
/// both YAML and serde on purpose: a leading `-` only at the front, at most one
/// `.` with digits on both sides, and no leading zero on a multi-digit integer
/// part. No exponent, no `+`, no hex, no underscores, no `Infinity`, no `NaN`.
/// Everything it refuses becomes a JSON string, which is how `01` stays `"01"`
/// instead of becoming 1.
fn is_numeric(value: &str) -> bool {
    let mut has_dot = false;
    let mut digit_seen = false;
    let mut int_len = 0usize;
    let mut int_first = b'0';
    for (index, byte) in value.bytes().enumerate() {
        if byte == b'-' && index == 0 {
            continue;
        }
        if byte == b'.' && !has_dot {
            if int_len == 0 {
                return false;
            }
            has_dot = true;
            continue;
        }
        if !byte.is_ascii_digit() {
            return false;
        }
        digit_seen = true;
        if !has_dot {
            if int_len == 0 {
                int_first = byte;
            }
            int_len += 1;
        }
    }
    digit_seen && !value.ends_with('.') && !(int_len > 1 && int_first == b'0')
}

#[cfg(test)]
mod tests {
    use super::is_numeric;

    /// Every spelling `isNumeric` accepts and a sample of what it refuses.
    ///
    /// These are the SPELLINGS under test, not values a constant could stand
    /// for: `1000` proves a multi-digit integer is not mistaken for a leading
    /// zero, and `1_000` proves a Rust-flavoured separator is refused where a
    /// naive `parse::<i64>()` would take it. Naming either would test the name.
    #[test]
    fn is_numeric_agrees_with_its_zig_original() {
        // pin test: literal is the contract
        for accepted in ["0", "5", "-5", "1.25", "1.00", "0.5", "-0.5", "1000"] {
            assert!(is_numeric(accepted), "{accepted} should be numeric");
        }
        for refused in [
            // pin test: literal is the contract
            "", "-", ".", "1.", ".5", "01", "1e5", "+1", "0x1F", "1_000", "NaN", "1.2.3", "5-",
        ] {
            assert!(!is_numeric(refused), "{refused} should not be numeric");
        }
    }
}
