//! Byte equality whose time does not depend on where two values first differ.
//!
//! RULE CTM in `.orly/standards/domains/auth.md`: a byte-by-byte `==` over a
//! secret, or over a digest that stands in for one, returns at the first
//! difference, and how long it took leaks how much of a guess was right.

use aws_lc_rs::constant_time::verify_slices_are_equal;

/// Whether `a` and `b` hold the same bytes.
///
/// Two values of different lengths answer `false` at once: a length is
/// public, and what must not leak is how many bytes of the content matched.
/// Values of one length are compared by `CRYPTO_memcmp`, in time independent of
/// where they differ.
#[must_use]
pub fn eq(a: &[u8], b: &[u8]) -> bool {
    verify_slices_are_equal(a, b).is_ok()
}
