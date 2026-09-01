//! `GET /v1/models` — the priced catalogue, served conditionally.
//!
//! The port of `model_library.zig`'s read half: the bounds, the normalized
//! provider filter, the struct cursor with its two distinct refusals, and the
//! validators on both answers. The revision-keyed response CACHE is the one
//! piece deliberately not ported — the milestone's Discovery log carries the
//! decision — so every request here builds its page; the wire is identical.
//!
//! The page is serialized ONCE: those bytes are what the `ETag` hashes and what
//! the response writes, so the tag and the body cannot disagree about a
//! page's identity by formatting it differently.

use std::borrow::Cow;
use std::sync::Arc;

use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_tenant::models::cursor::{self, Cursor};
use afd_tenant::models::{Boundary, LibraryPage, LibraryRow};
use afd_wire::models::{CatalogueModel, CatalogueResponse};
use axum::extract::{RawQuery, State};
use axum::response::{IntoResponse as _, Response};
use http::{HeaderMap, HeaderValue, StatusCode, header};

use crate::auth::PersonIdentity;
use crate::etag;
use crate::handler::Refusal;
use crate::services::{ModelCatalogue as _, Services};

/// The scoped event this read's failures are logged under.
const EVENT_CATALOGUE: &str = "model_catalogue_failed";

/// The page a caller naming no `limit` gets, and the most it may ask for.
const CATALOGUE_LIMIT_DEFAULT: u32 = 50;
const CATALOGUE_LIMIT_MAX: u32 = 100;

/// The most bytes a normalized `provider` filter may carry.
const PROVIDER_MAX_BYTES: usize = 128;

/// The refusal a `limit` outside `1..=100` — or not a number — earns.
pub const DETAIL_CATALOGUE_LIMIT: &str = "limit must be an integer between 1 and 100";

/// The refusal an oversized or unreadable `provider` filter earns.
pub const DETAIL_PROVIDER_BOUNDS: &str =
    "provider must be at most 128 bytes once normalized, and valid UTF-8";

/// The refusal a query string this daemon cannot decode earns.
pub const DETAIL_QUERY_UNREADABLE: &str = "Query string could not be parsed";

/// The refusal a token this endpoint never issued earns.
pub const DETAIL_CURSOR_MALFORMED: &str = "starting_after is not a cursor this endpoint issued";

/// The refusal a real cursor for a different query earns.
pub const DETAIL_CURSOR_MISMATCH: &str =
    "starting_after was issued for different filters or page size";

/// `private` because the response is authorized per caller even though the
/// payload is identical for all of them; `no-cache` means "store it, but
/// revalidate before reuse", which is what makes the `ETag` load-bearing.
const CACHE_CONTROL_VALUE: &str = "private, no-cache";
const VARY_VALUE: &str = "Authorization";

/// `GET /v1/models` — one page of the catalogue, as 200 or a bodyless 304.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/models",
    tag = afd_http::openapi::tag::MODEL_LIBRARY,
    operation_id = "get_model_library",
    summary = "List available models",
    description = concat!(
        "Returns models available to the tenant. Each item includes its ",
        "provider, context limit, and usage fields. An empty library returns ",
        "an empty array. A bounded keyset page ordered by normalized ",
        "`model_id`, then normalized `provider`, then an opaque row identity. ",
        "Every comparison runs under `COLLATE \"C\"`, so the order is byte-wise ",
        "and stable across locales. Follow `next_cursor` to read the whole ",
        "catalogue; a client that reads only the first page silently loses ",
        "every model past it. The response is conditionally revalidated. ",
        "Every answer carries an `ETag` over the exact bytes served, plus ",
        "`Cache-Control: private, no-cache` and `Vary: Authorization`. Send ",
        "the tag back as `If-None-Match` and a match answers `304` with no ",
        "body and the same headers. `private` is what stops a shared proxy ",
        "handing one tenant's response to another even though the payload is ",
        "identical for every authorized caller. ",
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK),
        (status = 304, description = afd_http::openapi::NOT_MODIFIED),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn catalogue<D: Services>(
    State(services): State<Arc<D>>,
    _identity: PersonIdentity,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Result<Response, Refusal> {
    let query = query.unwrap_or_default();
    let limit = parse_limit(decoded(&query, "limit")?)?;
    let provider = normalize_provider(decoded(&query, "provider")?)?;
    let after = parse_cursor(
        decoded(&query, "starting_after")?,
        provider.as_deref(),
        limit,
    )?;

    let page = services
        .catalogue()
        .page(provider.as_deref(), after.as_ref(), limit)
        .await
        .map_err(Refusal::at(EVENT_CATALOGUE))?;

    let body = serialize(&page, provider.as_deref(), limit);
    Ok(respond(&headers, body))
}

/// The page, serialized once into the bytes both the tag and the body use.
fn serialize(page: &LibraryPage, provider: Option<&str>, limit: u32) -> Vec<u8> {
    let next_cursor = page
        .has_more
        .then_some(page.boundary.as_ref())
        .flatten()
        .map(|boundary| {
            Cow::Owned(cursor::render(&Cursor {
                v: cursor::CURSOR_VERSION,
                display_key: boundary.display_key.clone(),
                vendor_key: boundary.vendor_key.clone(),
                id: boundary.id.clone(),
                provider: provider.map(str::to_owned),
                limit,
            }))
        });
    let response = CatalogueResponse {
        version: Cow::Owned(version_stamp(page.max_updated_ms)),
        models: page.models.iter().map(model).collect(),
        // Never counted — the key stays, as the guidelines require.
        total: None,
        next_cursor,
    };
    // Serializing strings and integers cannot fail; empty on the unreachable
    // arm beats an error branch no caller could act on.
    serde_json::to_vec(&response).unwrap_or_default()
}

/// One row as the wire shows it.
fn model(row: &LibraryRow) -> CatalogueModel<'_> {
    CatalogueModel {
        id: Cow::Borrowed(&row.model_id),
        provider: Cow::Borrowed(&row.provider),
        context_cap_tokens: row.context_cap_tokens,
        input_nanos_per_mtok: row.input_nanos_per_mtok,
        cached_input_nanos_per_mtok: row.cached_input_nanos_per_mtok,
        output_nanos_per_mtok: row.output_nanos_per_mtok,
    }
}

/// Writes the page with its validators, honouring a conditional read.
///
/// The validators ride BOTH answers: a 304 that omitted them would tell a
/// cache to stop revalidating the very representation it just revalidated.
fn respond(headers: &HeaderMap, body: Vec<u8>) -> Response {
    let tag = etag::compute(&[Some(&body)]);
    let validators = [
        (header::ETAG, tag.clone()),
        (header::CACHE_CONTROL, CACHE_CONTROL_VALUE.to_owned()),
        (header::VARY, VARY_VALUE.to_owned()),
    ];
    let revalidated = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|candidate| etag::matches_if_none_match(candidate, &tag));

    let mut response = if revalidated {
        StatusCode::NOT_MODIFIED.into_response()
    } else {
        (
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            )],
            body,
        )
            .into_response()
    };
    for (name, value) in validators {
        if let Ok(value) = HeaderValue::from_str(&value) {
            response.headers_mut().insert(name, value);
        }
    }
    response
}

/// The catalogue's version stamp: the newest row's change date, `YYYY-MM-DD`
/// UTC. An empty catalogue yields 0 → `1970-01-01`, a valid not-yet-
/// provisioned state rather than an error.
fn version_stamp(max_updated_ms: i64) -> String {
    const MS_PER_SECOND: i64 = 1000;
    const SECONDS_PER_DAY: i64 = 86_400;
    let seconds = (max_updated_ms / MS_PER_SECOND).max(0);
    let days = seconds / SECONDS_PER_DAY;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Gregorian date from days since the epoch — Howard Hinnant's
/// `civil_from_days`, the constant-time algorithm every date library uses.
/// Signed throughout, so no narrowing cast has to argue about a range.
const fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// The page size the caller asked for — absent OR EMPTY means the default,
/// and anything else outside `1..=100` earns the library bounds refusal.
fn parse_limit(raw: Option<Cow<'_, str>>) -> Result<u32, Refusal> {
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
fn normalize_provider(raw: Option<Cow<'_, str>>) -> Result<Option<String>, Refusal> {
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
fn parse_cursor(
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
fn decoded<'q>(query: &'q str, name: &str) -> Result<Option<Cow<'q, str>>, Refusal> {
    crate::handler::decoded_parameter(query, name).map_err(|_broken| {
        Refusal::coded(
            error_code::LIBRARY_INPUT_OUT_OF_BOUNDS,
            DETAIL_QUERY_UNREADABLE,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::version_stamp;

    #[test]
    fn the_version_stamp_renders_epoch_ms_as_a_utc_date() {
        // The seed timestamp `model_library_page.zig` pins: 2025-04-29 UTC.
        assert_eq!(version_stamp(1_745_884_800_000), "2025-04-29");
    }

    #[test]
    fn an_empty_or_pre_epoch_catalogue_clamps_to_the_epoch_date() {
        assert_eq!(version_stamp(0), "1970-01-01");
        assert_eq!(version_stamp(-1), "1970-01-01");
    }

    #[test]
    fn a_leap_day_survives_the_civil_arithmetic() {
        // 2024-02-29 00:00 UTC — the branchy corner of any date algorithm.
        assert_eq!(version_stamp(1_709_164_800_000), "2024-02-29");
    }
}
