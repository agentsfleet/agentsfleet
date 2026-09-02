//! What a fleet remembers, over HTTP: read a page of it, or forget one.
//!
//! The port of `http/handlers/memory/handler.zig`. Everything that turns a URL
//! into values is [`super::memory_request`]; everything that turns rows into a
//! reply is here.
//!
//! # Two routes, two capabilities, and no store verb between them
//!
//! `GET .../memories` takes `fleet:read` and `DELETE .../memories/{key}` takes
//! `fleet:write` — forgetting mutates what the fleet knows, and it is not a
//! lifecycle transition, so it is not `fleet:admin`. There is no POST: the
//! tenant store verb was retired with the runner-push cutover, so a fleet
//! remembers what it LEARNED and never what a caller asserted. The route table
//! serves no POST on this template, so the collection answers 405 without an
//! arm here saying so.
//!
//! # The ownership check is the store's, not this file's
//!
//! `memory.memory_entries` has no workspace column, so scoping it is a read of
//! `core.fleets` under a different role — which is why it lives in
//! [`afd_fleet::memory::operator`] beside the statements rather than as an
//! opening call every handler has to remember, the shape `helpers.zig` has.
//! [`WorkspaceContext`] here is this handler saying WHICH workspace it acts in,
//! never deciding whether it may.

use std::borrow::Cow;
use std::sync::Arc;

use afd_core::paging::Cursor;
use afd_fleet::memory::page::{After, Entry};
use afd_wire::memory::{MemoriesResponse, MemoryEntry};
use axum::Json;
use axum::extract::{Path, RawQuery, State};
use axum::response::{IntoResponse as _, Response};
use http::{StatusCode, Uri};

use crate::auth::WorkspaceContext;
use crate::handler::Refusal;
use crate::services::{FleetMemories as _, Services};

use super::detail::{FleetPath, parse_fleet_id};
use super::memory_request::{Read, memory_key};

/// The scoped events each verb's failures are logged under.
const EVENT_LIST: &str = "memory_list_failed";
const EVENT_FORGET: &str = "memory_forget_failed";

/// The event a search that matched nothing leaves.
const EVENT_SEARCH_ZERO_HIT: &str = "memory_search_zero_hit";

/// `GET /v1/workspaces/{workspace_id}/fleets/{fleet_id}/memories`.
///
/// One page, newest first, of everything the fleet remembers — or of one
/// category, or of what a free-text search matched. Which of the three is
/// [`Read::view`]'s answer, resolved once from the query string.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/memories",
    tag = afd_http::openapi::tag::MEMORY,
    operation_id = "list_fleet_memories",
    summary = "List or search memory entries for a fleet",
    description = concat!(
        "Returns memory entries for the fleet, newest-created first. Without ",
        "`query` this lists entries (optionally filtered by `category`). With ",
        "`query`, performs case-insensitive LIKE search across both `key` and ",
        "`content` (LIKE metacharacters in `query` are escaped — a query of ",
        "`%` does NOT return every entry). Every shape pages by keyset: pass ",
        "the previous response's `next_cursor` as `starting_after` to ",
        "continue the walk. A null `next_cursor` marks the final page. ",
    ),
    params(
        afd_http::openapi::path::Fleet,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = MemoriesResponse),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn list<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    Path(FleetPath { fleet_id }): Path<FleetPath>,
    RawQuery(query): RawQuery,
) -> Result<Response, Refusal> {
    // The query string is judged BEFORE the path, which is `innerListMemories`'
    // order: `parseListParams` runs first and `resolveFleetInWorkspace` — where
    // the identifier check lives — only after. A request wrong in both halves is
    // told about the same one by either daemon.
    let read = Read::parse(&query.unwrap_or_default())?;
    let fleet = parse_fleet_id(&fleet_id)?;
    let view = read.view();
    let after = read.after.as_ref().map(|boundary| After {
        created_at_ms: boundary.created_at_ms,
        key: &boundary.key,
    });

    let entries = services
        .memories()
        .page(&owned.workspace, &fleet, view, after, read.limit)
        .await
        .map_err(Refusal::at(EVENT_LIST))?;

    log_zero_hit(
        view.is_recall() && read.after.is_none() && entries.is_empty(),
        &fleet,
    );

    let next_cursor = next_cursor(&entries, read.limit);
    Ok(Json(MemoriesResponse {
        items: entries.iter().map(item).collect(),
        total: entries.len(),
        next_cursor: next_cursor.map(Cow::Owned),
    })
    .into_response())
}

fn log_zero_hit(zero_hit: bool, fleet: &afd_core::id::Uuid7) {
    if zero_hit {
        // A search path can be perfectly healthy and useless: this is the one
        // family that says a fleet's memory is not being reached.
        afd_observability::producers::memory::search_found_nothing();
        let fleet_field = fleet.as_str();
        tracing::debug!(
            fleet_id = fleet_field,
            event = EVENT_SEARCH_ZERO_HIT,
            "a memory search matched nothing the fleet is holding"
        );
    }
}

/// `DELETE /v1/workspaces/{workspace_id}/fleets/{fleet_id}/memories/{key}`.
///
/// The operator's correction path: a fleet learned a convention wrong, and the
/// entry has to go before the next hydrate seeds it into another run.
///
/// Takes the raw [`Uri`] rather than the key as a `Path` segment, and that is a
/// refusal being preserved rather than a preference — see
/// [`super::memory_request`] on what an extractor absorbs before the handler
/// can refuse it.
#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/memories/{key}",
    tag = afd_http::openapi::tag::MEMORY,
    operation_id = "delete_fleet_memory",
    summary = "Forget a fleet memory entry",
    description = concat!(
        "Deletes one memory entry for this fleet. The key is scoped to the ",
        "fleet, so a key from another fleet is not deleted. Missing keys ",
        "return 404 so an operator can tell a mistyped key from a successful ",
        "forget. ",
    ),
    params(
        afd_http::openapi::path::Memory,
    ),
    responses(
        (status = 204, description = afd_http::openapi::NO_CONTENT),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn forget<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    Path(FleetPath { fleet_id }): Path<FleetPath>,
    uri: Uri,
) -> Result<Response, Refusal> {
    // The key before the path's fleet, which is `innerDeleteMemory`'s order:
    // it decodes the segment first and reaches `resolveFleetInWorkspace` only
    // after. Same reason as the list's inverse ordering above.
    let key = memory_key(uri.path())?;
    let fleet = parse_fleet_id(&fleet_id)?;

    services
        .memories()
        .forget(&owned.workspace, &fleet, &key)
        .await
        .map_err(Refusal::at(EVENT_FORGET))?;

    // 204 with a genuinely empty body. A `Json(())` would put two bytes on a
    // status RFC 9110 forbids a body on, which some proxies reject and others
    // quietly normalise.
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Where the next page resumes, or `None` on the last one.
///
/// A FULL page means the walk may continue, and the boundary is the last row's
/// `(created_at, key)`. This surface cannot over-fetch the way the fleets list
/// does — `LIMIT` is the caller's own number and there is no spare row to peek
/// with — so a caller who asks for exactly as many entries as remain spends one
/// more request to learn there are none. That is `handler.zig`'s behaviour and
/// a client walking either daemon sees the same page sequence.
fn next_cursor(entries: &[Entry], limit: i64) -> Option<String> {
    let full = usize::try_from(limit).is_ok_and(|asked| entries.len() == asked);
    full.then(|| entries.last()).flatten().map(|last| {
        Cursor::Timestamp {
            at_ms: last.created_at_ms,
            id: last.key.clone(),
        }
        .to_string()
    })
}

/// One stored entry as the wire shows it.
///
/// `created_at` is deliberately absent: it orders the walk and feeds the
/// cursor, and putting it on the wire would invite a client to page on it
/// itself rather than on the opaque token this daemon issues.
fn item(entry: &Entry) -> MemoryEntry<'_> {
    MemoryEntry {
        key: Cow::Borrowed(&entry.key),
        content: Cow::Borrowed(&entry.content),
        category: Cow::Borrowed(&entry.category),
        updated_at: entry.updated_at_ms,
    }
}
