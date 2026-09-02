//! Reading `GET /v1/models`'s query string: the bounds, the normalized
//! filter, and the cursor with its two distinct refusals.
//!
//! Split from the handler beside it because parsing a request and serving one
//! are separate concerns that change for separate reasons — and because the
//! handler had no headroom left for the tests these functions deserve.

use std::borrow::Cow;

use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_tenant::models::Boundary;
use afd_tenant::models::cursor;

use super::{
    CATALOGUE_LIMIT_DEFAULT, CATALOGUE_LIMIT_MAX, DETAIL_CATALOGUE_LIMIT, DETAIL_CURSOR_MALFORMED,
    DETAIL_CURSOR_MISMATCH, DETAIL_PROVIDER_BOUNDS, DETAIL_QUERY_UNREADABLE, PROVIDER_MAX_BYTES,
};
use crate::handler::Refusal;

/// The page size the caller asked for — absent OR EMPTY means the default,
/// and anything else outside `1..=100` earns the library bounds refusal.
pub(super) fn parse_limit(raw: Option<Cow<'_, str>>) -> Result<u32, Refusal> {
    let Some(raw) = raw else {
        return Ok(CATALOGUE_LIMIT_DEFAULT);
    };
    if raw.is_empty() {
        return Ok(CATALOGUE_LIMIT_DEFAULT);
    }
    let limit: u32 = raw.parse().map_err(|_not_numeric| {
        Refusal::coded(
            error_code::LIBRARY_INPUT_OUT_OF_BOUNDS,
            DETAIL_CATALOGUE_LIMIT,
        )
    })?;
    if limit == 0 || limit > CATALOGUE_LIMIT_MAX {
        return Err(Refusal::coded(
            error_code::LIBRARY_INPUT_OUT_OF_BOUNDS,
            DETAIL_CATALOGUE_LIMIT,
        ));
    }
    Ok(limit)
}

/// The normalized provider filter: trimmed, interior whitespace collapsed,
/// ASCII-lowercased. Empty means ABSENT — `?provider=` is the same request
/// as omitting it — and only the byte bound refuses.
pub(super) fn normalize_provider(raw: Option<Cow<'_, str>>) -> Result<Option<String>, Refusal> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let is_space = |c: char| matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c');
    let trimmed = raw.trim_matches(is_space);
    if trimmed.is_empty() {
        return Ok(None);
    }
    let mut normalized = String::with_capacity(trimmed.len());
    let mut pending_space = false;
    for character in trimmed.chars() {
        if is_space(character) {
            pending_space = true;
            continue;
        }
        if pending_space {
            normalized.push(' ');
            pending_space = false;
        }
        normalized.push(character.to_ascii_lowercase());
    }
    if normalized.len() > PROVIDER_MAX_BYTES {
        return Err(Refusal::coded(
            error_code::LIBRARY_INPUT_OUT_OF_BOUNDS,
            DETAIL_PROVIDER_BOUNDS,
        ));
    }
    Ok(Some(normalized))
}

/// The decoded boundary, or one of the two DISTINCT cursor refusals.
///
/// A token that will not decode is `UZ-LIBRARY-001` — not something this
/// endpoint issued. One that decodes but names a different filter or page
/// size is `UZ-LIBRARY-002` — a real cursor for a different query. Folding
/// them would hide a filter change inside the same signal as a truncated URL.
/// Nothing is trusted from the cursor except the sort boundary: the filters
/// used for the read are always the request's, never the cursor's.
pub(super) fn parse_cursor(
    raw: Option<Cow<'_, str>>,
    provider: Option<&str>,
    limit: u32,
) -> Result<Option<Boundary>, Refusal> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    if raw.is_empty() {
        return Ok(None);
    }
    let malformed = || {
        Refusal::coded(
            error_code::LIBRARY_CURSOR_MALFORMED,
            DETAIL_CURSOR_MALFORMED,
        )
    };
    let token = cursor::parse(&raw).map_err(|_foreign| malformed())?;
    if token.limit != limit || token.provider.as_deref() != provider {
        return Err(Refusal::coded(
            error_code::LIBRARY_CURSOR_MISMATCH,
            DETAIL_CURSOR_MISMATCH,
        ));
    }
    // The id rides the page SQL as a `::uuid` cast, so a hand-minted cursor
    // whose id is not an identifier must fail HERE as the malformed input it
    // is — not downstream as a Postgres cast error dressed in a 500.
    if Uuid7::parse(&token.id).is_err() {
        return Err(malformed());
    }
    Ok(Some(Boundary {
        display_key: token.display_key,
        vendor_key: token.vendor_key,
        id: token.id,
    }))
}

/// The shared decode, under this family's unreadable-query sentence — the
/// Zig handler answers `UZ-LIBRARY-003` for a query string it cannot read.
pub(super) fn decoded<'q>(query: &'q str, name: &str) -> Result<Option<Cow<'q, str>>, Refusal> {
    crate::handler::decoded_parameter(query, name).map_err(|_broken| {
        Refusal::coded(
            error_code::LIBRARY_INPUT_OUT_OF_BOUNDS,
            DETAIL_QUERY_UNREADABLE,
        )
    })
}
