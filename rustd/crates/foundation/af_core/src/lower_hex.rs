//! The one test for a lowercase hexadecimal digit.
//!
//! Every digest, tag and identifier this daemon writes is lowercase hex, and
//! every reader that checks one refuses `A-F` rather than folding it: a value
//! with two spellings is two cache keys, two path segments, two names for one
//! thing. `char::is_ascii_hexdigit` is the obvious call and the wrong one,
//! because it accepts uppercase — which is why the readers share this instead
//! of each writing the range again.

/// Whether `digit` is `0`-`9` or `a`-`f`.
///
/// A per-character test, so a caller owns its own length rule: a garde field
/// declares `length` and `custom(charset(is_digit))` as two rules, and a whole
/// string check pairs its own width with `.chars().all(is_digit)`. A byte
/// reader passes `char::from(byte)`, which maps every non-ASCII byte to a
/// character outside the range.
#[must_use]
pub const fn is_digit(digit: char) -> bool {
    matches!(digit, '0'..='9' | 'a'..='f')
}

#[cfg(test)]
mod tests {
    use super::is_digit;

    #[test]
    fn every_lowercase_hex_digit_is_one() {
        assert!("0123456789abcdef".chars().all(is_digit));
    }

    #[test]
    fn uppercase_and_every_neighbour_of_the_range_is_refused() {
        for refused in ['A', 'F', 'g', '/', ':', '`', ' ', '\0', 'é'] {
            assert!(!is_digit(refused), "{refused:?}");
        }
        assert!(!is_digit(char::from(0xe1_u8)), "a non-ASCII byte");
    }
}
