//! `/v1/workspaces/{workspace_id}/fleet-libraries` — browse and onboard.
//!
//! `GET` is the gallery: the published platform catalogue and this workspace's
//! own entries as one merged page. `POST` onboards a bundle into the workspace's
//! own library, over the same pipeline the operator's catalogue uses.
//!
//! # Nothing here decides who may act
//!
//! [`WorkspaceContext`] is the ownership boundary and it runs before either
//! verb: a caller reaching this file has already been proven to own the
//! workspace the path names. The store's own predicates are the second half —
//! every statement filters on that workspace, so neither layer alone is what
//! keeps one tenant's gallery out of another's.
//!
//! Being an EXTRACTOR is also a declared divergence. The Zig checks the page
//! bounds and the cursor before it authorizes, so a caller who owns neither the
//! workspace nor a valid `limit` is told about the limit; here ownership
//! answers first. That is the safer order — input-validation behaviour is not
//! something a non-owner should be able to probe — and it is the order every
//! other workspace route in this daemon already uses, which matters more than
//! matching a sequence whose only observable difference is which refusal a
//! caller who is refused either way receives.
//!
//! # The cursor is bound to the walk that issued it
//!
//! A gallery token carries the WORKSPACE and the page size it was minted under,
//! and one naming either differently is refused as `UZ-LIBRARY-002`. The
//! workspace arm is the one that matters: it is what stops a cursor minted in
//! one workspace from seeking inside another. Nothing is trusted from the token
//! except the sort boundary — the workspace read is always the path's.

use std::borrow::Cow;
use std::sync::Arc;
use std::time::Instant;

use afd_observability::metrics::label::library::{ReadOutcome, Stage, Surface};
use afd_observability::producers::library;

use afd_core::error_code;
use afd_core::paging::QUERY_STARTING_AFTER;
use afd_core::paging::struct_cursor::{self, StructCursor};
use afd_library::{Destination, GalleryPage, Onboarded, Position, SummaryEntry, Tier};
/// Named only by the `body =` clause of this module's `utoipa::path`
/// annotations, which the default build compiles away — so the import has to
/// go with them or the feature-off build fails on an unused name.
#[cfg(feature = "openapi")]
use afd_wire::admin::AdminLibraryCreated;
use afd_wire::admin::AdminLibraryRequirements;
use afd_wire::workspace_library::{GalleryCard, GalleryResponse};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{RawQuery, State};
use axum::response::{IntoResponse as _, Response};
use http::StatusCode;
use serde::{Deserialize, Serialize};

use crate::auth::WorkspaceContext;
use crate::handler::paging::requested_limit;
use crate::handler::{Refusal, library_onboard, parameter};
use crate::services::Services;

// A sentence the catalogue page already owns, imported rather than respelled
// (RULE UFS): an unissued token is the same fact here, and a caller told two
// different things about one rule looks for two mistakes. The page bound is
// shared the same way, one level up in `handler::paging`. What is NOT shared is
// the mismatch below, because the three pages bind a cursor to three different
// things.
use super::tenant::DETAIL_CURSOR_MALFORMED;

/// The scoped events each verb's failures are logged under.
const EVENT_GALLERY: &str = "workspace_library_list_failed";
const EVENT_ONBOARD: &str = "workspace_library_onboard_failed";

/// The refusal a real cursor for a different walk earns.
///
/// Its own sentence: this page binds a token to the WORKSPACE it was issued
/// for, where the registry binds one to the tenant and the catalogue to its
/// filters.
pub const DETAIL_CURSOR_MISMATCH: &str =
    "starting_after was issued for a different workspace or page size";

/// This page's cursor payload, in the Zig's fixed key order.
///
/// Carries all three parts of the compound order, because all three are needed
/// to place a row in it — plus the workspace and limit the walk was issued
/// under. Field ORDER is the canonical key order, so reordering this
/// declaration invalidates every token already in flight.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    /// The payload generation this cursor was issued under.
    v: u8,
    /// The boundary row's creation instant.
    created_at: i64,
    /// Which library the boundary row came from, as its sort rank.
    tier_rank: i32,
    /// The boundary row's identifier, compared bytewise.
    id: String,
    /// The workspace the walk was issued for.
    workspace_uuid: String,
    /// The page size the walk was issued under.
    limit: u32,
}

impl StructCursor for Cursor {
    fn generation(&self) -> u8 {
        self.v
    }
}

/// `GET /v1/workspaces/{workspace_id}/fleet-libraries` — one page of the gallery.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/workspaces/{workspace_id}/fleet-libraries",
    tag = afd_http::openapi::tag::FLEET_LIBRARY,
    operation_id = "list_workspace_fleet_library",
    summary = "List the workspace Fleet library gallery",
    description = concat!(
        "Returns the union of the platform Fleet library catalog and this ",
        "workspace's own tenant entries — and nothing from another workspace. ",
        "Each entry carries identity, source, requirements and per-credential ",
        "reason copy; never an object-store key. A bounded keyset page ",
        "ordered by `created_at DESC, tier ASC, id DESC` across BOTH ",
        "catalogs. Follow `next_cursor` to read the whole gallery; a client ",
        "that reads only the first page silently loses every entry past it. ",
        "The support-file manifest is stored on import but served by no ",
        "endpoint. ",
    ),
    params(
        afd_http::openapi::path::Workspace,
        ("limit" = Option<String>, Query, description = "Rows per page, 1..100. Defaults to 50."),
        ("starting_after" = Option<String>, Query, description = "Opaque cursor from a previous page's `next_cursor`. Bound to the workspace and page size that produced it; a mismatch is `UZ-LIBRARY-002` rather than a silently different page."),
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = GalleryResponse),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn list<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    RawQuery(query): RawQuery,
) -> Result<Response, Refusal> {
    let read = read_gallery(&services, &owned, query).await;
    library::read_finished(
        SURFACE,
        read.as_ref()
            .map_or_else(afd_http::handler::library_outcome, |_served| {
                ReadOutcome::Ok
            }),
    );
    read
}

/// The surface this read serves, in the census's own spelling.
const SURFACE: Surface = Surface::FleetSummary;

/// [`list`] without the outcome recording, so there is one place the answer is
/// produced and one place it is classified.
async fn read_gallery<D: Services>(
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
            .gallery(&owned.workspace, limit, after.as_ref()),
    )
    .await
    .map_err(Refusal::at(EVENT_GALLERY))?;

    let rows = u64::try_from(page.items.len()).unwrap_or(u64::MAX);
    let serializing = Instant::now();
    let body = Json(rendered(&page, owned.workspace.as_str(), limit)).into_response();
    library::stage_observed(SURFACE, Stage::Serialize, serializing.elapsed());
    library::read_served(SURFACE, rows);

    Ok(body)
}

/// `POST /v1/workspaces/{workspace_id}/fleet-libraries` — onboard into it.
///
/// The same body, the same refusals and the same pipeline as the operator's
/// catalogue — see [`library_onboard`], which both planes parse through. What
/// this verb chooses is the destination.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/workspaces/{workspace_id}/fleet-libraries",
    tag = afd_http::openapi::tag::FLEET_LIBRARY,
    operation_id = "onboard_tenant_fleet_library",
    summary = "Onboard a tenant Fleet library entry",
    description = concat!(
        "Onboards a Fleet library entry into the caller's workspace catalog ",
        "from a GitHub source reference. Requires the `library:write` scope ",
        "plus ownership of the target workspace. Re-onboarding identical ",
        "bytes converges on one `(workspace_id, content_hash)` row. The ",
        "response carries metadata only. ",
    ),
    params(
        afd_http::openapi::path::Workspace,
    ),
    responses(
        (status = 201, description = afd_http::openapi::CREATED, body = AdminLibraryCreated),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn onboard<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    body: Bytes,
) -> Result<Response, Refusal> {
    // `replace_requested` is deliberately dropped: it belongs to the platform
    // catalogue, whose name a second source can collide with. This library is
    // keyed by content hash and `Destination::Workspace` has no field for it,
    // so the flag cannot be acted on here even by mistake.
    let library_onboard::Parsed { onboarding, .. } =
        library_onboard::parse(&body).map_err(|(code, detail)| Refusal::coded(code, detail))?;

    let onboarded = library_onboard::run(
        services.library_imports(),
        onboarding,
        Destination::Workspace(&owned.workspace),
        services.now(),
    )
    .await
    .map_err(Refusal::at(EVENT_ONBOARD))?;

    Ok((StatusCode::CREATED, Json(created(onboarded))).into_response())
}

/// The boundary this request resumes from, or nothing for the first page.
///
/// The identity check is here and not in the store: only this function knows
/// which workspace the path named and which limit was asked for, which is the
/// whole reason the store takes a [`Position`] rather than a token.
fn resume_from(raw: &str, workspace: &str, limit: u32) -> Result<Option<Position>, Refusal> {
    let Some(token) = parameter(raw, QUERY_STARTING_AFTER).filter(|token| !token.is_empty()) else {
        return Ok(None);
    };
    let malformed = || {
        Refusal::coded(
            error_code::LIBRARY_CURSOR_MALFORMED,
            DETAIL_CURSOR_MALFORMED,
        )
    };
    let cursor: Cursor = struct_cursor::parse(token).map_err(|_foreign| malformed())?;
    if cursor.workspace_uuid != workspace || cursor.limit != limit {
        return Err(Refusal::coded(
            error_code::LIBRARY_CURSOR_MISMATCH,
            DETAIL_CURSOR_MISMATCH,
        ));
    }
    // A rank this build cannot name is not a boundary it can seek from, and it
    // is a token this endpoint did not issue — the same answer a corrupt one
    // gets, because to a caller the repair is the same.
    let tier = Tier::from_rank(cursor.tier_rank).ok_or_else(malformed)?;
    Ok(Some(Position {
        created_at_ms: cursor.created_at,
        tier,
        id: cursor.id,
    }))
}

/// The page, rendered.
fn rendered<'p>(page: &'p GalleryPage, workspace: &str, limit: u32) -> GalleryResponse<'p> {
    GalleryResponse {
        items: page.items.iter().map(card).collect(),
        // Always null: counting a keyset page costs the scan this pagination
        // exists to avoid, and the key stays present rather than vanishing.
        total: None,
        next_cursor: page.next.as_ref().map(|position| {
            struct_cursor::render(&Cursor {
                v: struct_cursor::VERSION,
                created_at: position.created_at_ms,
                tier_rank: position.tier.rank(),
                id: position.id.clone(),
                workspace_uuid: workspace.to_owned(),
                limit,
            })
        }),
    }
}

/// One card, rendered.
///
/// `visibility` is the TIER's label — see [`afd_wire::workspace_library`] on why
/// that field name carries a different fact here than on the admin surface.
fn card(entry: &SummaryEntry) -> GalleryCard<'_> {
    GalleryCard {
        id: Cow::Borrowed(&entry.id),
        name: Cow::Borrowed(&entry.name),
        description: Cow::Borrowed(&entry.description),
        visibility: Cow::Borrowed(entry.tier.label()),
        source_ref: Cow::Borrowed(&entry.source_ref),
        created_at: entry.created_at_ms,
        requirements: requirements(&entry.requirements),
        required_credentials_reasons: entry.required_credentials_reasons.clone(),
    }
}

/// What a bundle declares it needs, rendered.
///
/// Every name is BORROWED onto the wire. A page is up to a hundred cards and
/// each carries three lists, so copying them would be the one allocation on
/// this path that scales with the page.
fn requirements(declared: &afd_library::LibraryRequirements) -> AdminLibraryRequirements<'_> {
    AdminLibraryRequirements {
        credentials: borrowed(declared.credentials()),
        tools: borrowed(declared.tools()),
        network_hosts: borrowed(declared.network_hosts()),
        trigger_present: declared.trigger_present(),
    }
}

/// One declared list, borrowed rather than copied.
fn borrowed(names: &[String]) -> Vec<Cow<'_, str>> {
    names
        .iter()
        .map(|name| Cow::Borrowed(name.as_str()))
        .collect()
}

/// The onboarded entry, rendered.
///
/// The same shape the operator's catalogue answers with, because both verbs say
/// the same thing — which entry now stands — and the tier is what differs.
fn created(onboarded: Onboarded) -> afd_wire::admin::AdminLibraryCreated<'static> {
    let bundle = onboarded.bundle;
    let declared = bundle.requirements;
    afd_wire::admin::AdminLibraryCreated {
        id: Cow::Owned(onboarded.id),
        name: Cow::Owned(bundle.name),
        visibility: Cow::Borrowed(Tier::Tenant.label()),
        content_hash: Cow::Owned(bundle.content_hash),
        requirements: AdminLibraryRequirements {
            credentials: declared.credentials.into_iter().map(Cow::Owned).collect(),
            tools: declared.tools.into_iter().map(Cow::Owned).collect(),
            network_hosts: declared.network_hosts.into_iter().map(Cow::Owned).collect(),
            trigger_present: declared.trigger_present,
        },
    }
}

#[cfg(test)]
mod tests;
