//! `GET /v1/workspaces/{workspace_id}/library-entries` — one page of what a
//! workspace onboarded.
//!
//! Split from the removal beside it by what changes together: a field added to
//! an entry lands here and in `afd_wire::workspace_library`, and touches
//! nothing about who may remove one.

use std::borrow::Cow;
use std::sync::Arc;
use std::time::Instant;

use afd_core::error_code;
use afd_core::paging::QUERY_STARTING_AFTER;
use afd_core::paging::struct_cursor;
use afd_library::{EntryPosition, OwnedEntry, OwnedPage};
use afd_observability::metrics::label::library::{ReadOutcome, Stage};
use afd_observability::producers::library;
use afd_wire::workspace_library::{OwnedEntriesResponse, OwnedEntryCard};
use axum::Json;
use axum::extract::{RawQuery, State};
use axum::response::{IntoResponse as _, Response};

use super::{Cursor, EVENT_LIST, SURFACE};
use crate::auth::WorkspaceContext;
use crate::handler::paging::requested_limit;
use crate::handler::tenant::DETAIL_CURSOR_MALFORMED;
use crate::handler::workspace_library::DETAIL_CURSOR_MISMATCH;
use crate::handler::{Refusal, parameter};
use crate::services::Services;

/// `GET /v1/workspaces/{workspace_id}/library-entries` — the owned collection.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/workspaces/{workspace_id}/library-entries",
    tag = afd_http::openapi::tag::FLEET_LIBRARY,
    operation_id = "list_workspace_library_entries",
    summary = "List the Fleet library entries this workspace onboarded",
    description = concat!(
        "Returns only the entries this workspace onboarded — never a platform ",
        "catalog row, and never another workspace's. The gallery at ",
        "`/fleet-libraries` is what lists everything installable here; this is ",
        "what the workspace administers. Each entry carries its identity, its ",
        "provenance and the content hash of the bundle bytes it holds, which ",
        "is what tells two near-identical onboardings apart. Never bundle ",
        "content. A bounded keyset page ordered by `created_at DESC, id DESC`; ",
        "follow `next_cursor` to read the whole collection. ",
    ),
    params(
        afd_http::openapi::path::Workspace,
        ("limit" = Option<String>, Query, description = "Rows per page, 1..100. Defaults to 50."),
        ("starting_after" = Option<String>, Query, description = "Opaque cursor from a previous page's `next_cursor`. Bound to the workspace and page size that produced it; a mismatch is `UZ-LIBRARY-002`. A gallery cursor cannot be spent here."),
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = OwnedEntriesResponse),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn list<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    RawQuery(query): RawQuery,
) -> Result<Response, Refusal> {
    let read = read_owned(&services, &owned, query).await;
    library::read_finished(
        SURFACE,
        read.as_ref()
            .map_or_else(afd_http::handler::library_outcome, |_served| {
                ReadOutcome::Ok
            }),
    );
    read
}

/// [`list`] without the outcome recording, so there is one place the answer is
/// produced and one place it is classified.
async fn read_owned<D: Services>(
    services: &Arc<D>,
    owned: &afd_http::auth::Owned,
    query: Option<String>,
) -> Result<Response, Refusal> {
    let raw = query.unwrap_or_default();
    let limit = requested_limit(&raw)?;
    let after = resume_from(&raw, owned.workspace.as_str(), limit)?;

    let page = library::timed(
        SURFACE,
        Stage::Sql,
        services
            .libraries()
            .owned_entries(&owned.workspace, limit, after.as_ref()),
    )
    .await
    .map_err(Refusal::at(EVENT_LIST))?;

    let rows = u64::try_from(page.items.len()).unwrap_or(u64::MAX);
    let serializing = Instant::now();
    let body = Json(rendered(&page, owned.workspace.as_str(), limit)).into_response();
    library::stage_observed(SURFACE, Stage::Serialize, serializing.elapsed());
    library::read_served(SURFACE, rows);

    Ok(body)
}

/// The boundary this request resumes from, or nothing for the first page.
///
/// The identity check is here and not in the store: only this function knows
/// which workspace the path named and which limit was asked for. A cursor
/// naming either differently is refused rather than quietly seeking somewhere
/// else — the workspace arm is what stops a token minted in one workspace from
/// resuming inside another.
pub(super) fn resume_from(
    raw: &str,
    workspace: &str,
    limit: u32,
) -> Result<Option<EntryPosition>, Refusal> {
    let Some(token) = parameter(raw, QUERY_STARTING_AFTER).filter(|token| !token.is_empty()) else {
        return Ok(None);
    };
    let cursor: Cursor = struct_cursor::parse(token).map_err(|_foreign| {
        Refusal::coded(
            error_code::LIBRARY_CURSOR_MALFORMED,
            DETAIL_CURSOR_MALFORMED,
        )
    })?;
    if cursor.workspace_uuid != workspace || cursor.limit != limit {
        return Err(Refusal::coded(
            error_code::LIBRARY_CURSOR_MISMATCH,
            DETAIL_CURSOR_MISMATCH,
        ));
    }
    Ok(Some(EntryPosition {
        created_at_ms: cursor.created_at,
        id: cursor.id,
    }))
}

/// The page, rendered.
fn rendered<'p>(page: &'p OwnedPage, workspace: &str, limit: u32) -> OwnedEntriesResponse<'p> {
    OwnedEntriesResponse {
        items: page.items.iter().map(entry).collect(),
        // Always null: counting a keyset page costs the scan this pagination
        // exists to avoid, and the key stays present rather than vanishing.
        total: None,
        next_cursor: page.next.as_ref().map(|position| {
            struct_cursor::render(&Cursor {
                v: struct_cursor::VERSION,
                created_at: position.created_at_ms,
                id: position.id.clone(),
                workspace_uuid: workspace.to_owned(),
                limit,
            })
        }),
    }
}

/// One entry, rendered — borrowed, never copied.
fn entry(owned: &OwnedEntry) -> OwnedEntryCard<'_> {
    OwnedEntryCard {
        id: Cow::Borrowed(&owned.id),
        name: Cow::Borrowed(&owned.name),
        description: Cow::Borrowed(&owned.description),
        source_kind: Cow::Borrowed(&owned.source_kind),
        source_ref: Cow::Borrowed(&owned.source_ref),
        content_hash: Cow::Borrowed(&owned.content_hash),
        created_at: owned.created_at_ms,
    }
}
