//! Page-bound parsing shared by the tenant plane's keyset walks.
//!
//! One rule with more than one call site is exactly how `model_id` ended up
//! bounded on the catalogue route and unbounded on the registry one, and the
//! page size is the same shape of rule: the registry quad and the workspace
//! gallery walk different tables under different cursors, but the bound a
//! client is held to, and the sentence it is told when it misses, are one fact.
//! Spelling it twice is two places for a bound to drift from its own message.
//!
//! What is NOT here is `models.rs`'s own parse. That one reads an already
//! decoded [`Cow`](std::borrow::Cow) and treats an EMPTY `?limit=` as absent,
//! which is a different contract rather than a second copy of this one — the
//! catalogue is a public route where a client that always sends the parameter
//! should not be refused for leaving it blank.

use afd_core::error_code;
use afd_core::paging::{DEFAULT_LIMIT, MAX_LIMIT, QUERY_LIMIT};

use crate::handler::tenant::DETAIL_CATALOGUE_LIMIT;
use crate::handler::{Refusal, parameter};

/// The page size this request asked for, already bounded.
///
/// An absent `limit` is the default rather than a refusal, so a client that
/// never learned about paging still gets a page.
pub(crate) fn requested_limit(raw: &str) -> Result<u32, Refusal> {
    let Some(asked) = parameter(raw, QUERY_LIMIT) else {
        return Ok(DEFAULT_LIMIT);
    };
    asked
        .parse::<u32>()
        .ok()
        .filter(|limit| (1..=MAX_LIMIT).contains(limit))
        .ok_or_else(|| {
            Refusal::coded(
                error_code::LIBRARY_INPUT_OUT_OF_BOUNDS,
                DETAIL_CATALOGUE_LIMIT,
            )
        })
}
