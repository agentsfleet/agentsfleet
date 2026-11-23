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
//! The bound itself is `af_validate::Limit` against the deployment's list
//! page (`[api]`); what this module adds is the library family's code and
//! sentence.

use af_core::error_code;
use af_core::paging::QUERY_LIMIT;
use af_validate::{Ceiling, Limit};

use crate::handler::{Refusal, parameter};

/// The page size this request asked for, already bounded.
///
/// An absent or empty `limit` is the default rather than a refusal, so a client
/// that never learned about paging still gets a page.
pub(crate) fn requested_limit(raw: &str, ceiling: Ceiling) -> Result<u32, Refusal> {
    catalogue_limit(parameter(raw, QUERY_LIMIT), ceiling)
}

/// The library family's page size from a value already read off the query.
pub(crate) fn catalogue_limit(asked: Option<&str>, ceiling: Ceiling) -> Result<u32, Refusal> {
    Limit::parse(asked, ceiling).map_err(|_break| {
        Refusal::coded_sentence(
            error_code::LIBRARY_INPUT_OUT_OF_BOUNDS,
            afd_http::paging::limit_sentence("limit must be an integer", ceiling),
        )
    })
}
