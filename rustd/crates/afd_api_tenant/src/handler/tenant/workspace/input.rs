//! The list's query string, parsed once at the boundary into the types the
//! walk takes: a bounded limit, a cursor this daemon issued, an exact name.

use std::borrow::Cow;

use afd_core::id::Uuid7;
use afd_core::paging::{BoundaryKind, CEILING, Cursor};
use afd_tenant::workspace::directory::After;
use afd_validate::{Limit, nul_free};
use garde::Validate as _;

use crate::handler::Refusal;

/// The refusal a query string this daemon cannot decode earns.
pub const DETAIL_MALFORMED_QUERY: &str = "Malformed query string";

/// The refusal a `limit` outside `1..=100` — or not a number — earns.
///
/// ONE sentence for both, where the charges walk spells two: each surface keeps
/// its own published vocabulary, apart on purpose.
pub const DETAIL_INVALID_LIMIT: &str = "Limit must be between 1 and 100";

/// The refusal a `starting_after` this daemon never issued earns.
pub const DETAIL_INVALID_CURSOR: &str = "Invalid starting_after cursor";

/// The refusal an unusable `name` filter earns.
pub const DETAIL_INVALID_NAME: &str = "Name must be between 1 and 128 Unicode code points";

/// The most code points a `name` FILTER may carry — the stored cap's number,
/// restated here because the refusal sentence above names it (RULE UFS).
const NAME_FILTER_MAX_CODEPOINTS: usize = 128;

/// An exact-name filter as the caller sent it, decoded.
///
/// Bounds only — 1 to 128 code points, no NUL — because a FILTER that would
/// match nothing is the caller's business; the strict character rules belong
/// to the create, where a value is stored rather than compared.
#[derive(Debug, garde::Validate)]
struct NameFilter<'q> {
    #[garde(
        length(chars, min = 1, max = NAME_FILTER_MAX_CODEPOINTS),
        custom(nul_free)
    )]
    name: Cow<'q, str>,
}

/// The page size the caller asked for, or the one refusal any wrong spelling
/// earns — the directory does not say which way a limit was wrong.
///
/// The bound is the shared keyset one, [`CEILING`]; the charges walk allows
/// two hundred, `afd_billing::tenant::CHARGES_LIMIT_MAX`.
pub(super) fn requested_limit(raw: Option<&str>) -> Result<u32, Refusal> {
    Limit::parse(raw, CEILING).map_err(|_break| Refusal::malformed(DETAIL_INVALID_LIMIT))
}

/// The decoded boundary, or the refusal a foreign token earns.
///
/// The workspace walk's cursor is the `{created_at_ms}:{id}` form with a
/// workspace identifier in its second half — the text-sort form and a
/// non-identifier id are both tokens some OTHER list issued, refused here the
/// way `isSupportedWorkspaceId` refuses them.
pub(super) fn parse_cursor(raw: Option<Cow<'_, str>>) -> Result<Option<After>, Refusal> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let cursor =
        Cursor::parse(&raw).map_err(|_foreign| Refusal::malformed(DETAIL_INVALID_CURSOR))?;
    if cursor.kind() != BoundaryKind::Timestamp {
        return Err(Refusal::malformed(DETAIL_INVALID_CURSOR));
    }
    let id = Uuid7::parse(cursor.id())
        .map_err(|_not_workspace| Refusal::malformed(DETAIL_INVALID_CURSOR))?;
    let Cursor::Timestamp { at_ms, .. } = cursor else {
        // The kind was just checked; stated as unreachable rather than left
        // for a refactor to make reachable silently.
        return Err(Refusal::malformed(DETAIL_INVALID_CURSOR));
    };
    Ok(Some(After {
        created_at_ms: at_ms,
        id,
    }))
}

/// The exact-name filter, or the refusal an unusable one earns.
pub(super) fn parse_name(raw: Option<Cow<'_, str>>) -> Result<Option<String>, Refusal> {
    raw.map(|name| {
        let filter = NameFilter { name };
        filter
            .validate()
            .map_err(|_report| Refusal::malformed(DETAIL_INVALID_NAME))?;
        Ok(filter.name.into_owned())
    })
    .transpose()
}

/// One query parameter, percent-decoded — the shared scan, with this route's
/// refusal sentence when a broken escape refuses the whole query string.
pub(super) fn decoded<'q>(query: &'q str, name: &str) -> Result<Option<Cow<'q, str>>, Refusal> {
    crate::handler::decoded_parameter(query, name)
        .map_err(|_broken| Refusal::malformed(DETAIL_MALFORMED_QUERY))
}
