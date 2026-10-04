//! Page-bound reading shared by the tenant plane's keyset walks.
//!
//! One rule with more than one call site is exactly how `model_id` ended up
//! bounded on the catalogue route and unbounded on the registry one, and the
//! page size is the same shape of rule: the registry quad, the workspace
//! gallery and the catalogue walk different tables under different cursors,
//! but the bound a client is held to, and the sentence it is told when it
//! misses, are one fact. Spelling it twice is two places for a bound to drift
//! from its own message.
//!
//! The bound itself is `afd_validate::Limit` against the keyset lists'
//! [`CEILING`]; what this module adds is the library family's code and
//! sentence.

use afd_core::error_code;
use afd_core::paging::{CEILING, Ceiling, QUERY_LIMIT};
use afd_validate::Limit;

use crate::handler::tenant::DETAIL_CATALOGUE_LIMIT;
use crate::handler::{Refusal, parameter};

/// The page size this request asked for, already bounded.
///
/// An absent or empty `limit` is the default rather than a refusal, so a client
/// that never learned about paging still gets a page.
pub(crate) fn requested_limit(raw: &str) -> Result<u32, Refusal> {
    catalogue_limit(parameter(raw, QUERY_LIMIT))
}

/// The library family's page size from a value already read off the query.
pub(crate) fn catalogue_limit(asked: Option<&str>) -> Result<u32, Refusal> {
    Limit::parse(asked, CEILING).map_err(|_break| {
        Refusal::coded(
            error_code::LIBRARY_INPUT_OUT_OF_BOUNDS,
            DETAIL_CATALOGUE_LIMIT,
        )
    })
}

/// A route's ceiling from a store's signed page constants.
///
/// The event store spells its page sizes as `i64`, the type its statements
/// bind, and `TryFrom` cannot run in a `const`; the assertion is what makes
/// the narrowing below exact, and it runs at compile time.
///
/// # Panics
/// At compile time, when either value is not a positive `u32`.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the assertion above each cast proves both values are positive and inside u32"
)]
pub(crate) const fn store_ceiling(max: i64, default: i64) -> Ceiling {
    assert!(
        default > 0 && max <= u32::MAX as i64,
        "a store's page constants must be positive and fit a u32"
    );
    Ceiling::new(max as u32, default as u32)
}
