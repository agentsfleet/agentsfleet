//! The workspace directory over HTTP: the tenant's list, and the create.
//!
//! The port of `tenant_workspaces.zig` and `workspaces/lifecycle.zig`,
//! sentence for sentence — with one Discovery-logged divergence: a create
//! naming nothing gets a GENERATED name where the Zig daemon answers a 400,
//! because "create me a workspace" was never a naming decision.

use std::borrow::Cow;
use std::sync::Arc;

use afd_core::id::Uuid7;
use afd_core::paging::{BoundaryKind, Cursor};
use afd_tenant::workspace::directory::{After, Created, WorkspacePage, WorkspaceRow};
use afd_tenant::workspace::name::Chosen;
use afd_wire::workspace::{
    CreateWorkspaceRequest, CreatedWorkspaceResponse, WorkspaceSummary, WorkspacesResponse,
};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{RawQuery, State};
use axum::response::{IntoResponse as _, Response};
use http::StatusCode;

use afd_observability::Telemetry;

use crate::auth::PersonIdentity;
use crate::handler::Refusal;
use crate::request_id::RequestId;
use crate::services::{Services, TenantWorkspaces as _, WorkspaceOwnership as _};

use super::{DETAIL_TENANT_REQUIRED, tenant_of};

/// The scoped events each verb's failures are logged under.
const EVENT_LIST: &str = "workspace_list_failed";
const EVENT_CREATE: &str = "workspace_create_failed";
const EVENT_TENANT: &str = "workspace_tenant_unresolved";

/// The list page a caller naming no `limit` gets.
const LIST_LIMIT_DEFAULT: u32 = 50;

/// The most rows one list page may carry.
///
/// One hundred where the charges walk allows two hundred — each is its own
/// Zig handler's number, and parity keeps them apart.
const LIST_LIMIT_MAX: u32 = 100;

/// The refusal a query string this daemon cannot decode earns.
pub const DETAIL_MALFORMED_QUERY: &str = "Malformed query string";

/// The refusal a `limit` outside `1..=100` — or not a number — earns.
///
/// ONE sentence for both, where the charges walk spells two: each is its Zig
/// handler's own vocabulary, kept apart on purpose.
pub const DETAIL_INVALID_LIMIT: &str = "Limit must be between 1 and 100";

/// The refusal a `starting_after` this daemon never issued earns.
pub const DETAIL_INVALID_CURSOR: &str = "Invalid starting_after cursor";

/// The refusal an unusable `name` filter earns.
pub const DETAIL_INVALID_NAME: &str = "Name must be between 1 and 128 Unicode code points";

/// The refusal a create body this daemon cannot read earns.
pub const DETAIL_CREATE_BODY: &str = "Malformed JSON";

/// The create's refusal for a session resolving to no tenant.
///
/// A 401 where the list's is a 403 — `lifecycle.zig`'s split, kept: a list
/// caller lacks a context, a create caller's session has gone stale under it.
pub const DETAIL_CREATE_NO_TENANT: &str = "Missing tenant context on session";

/// The state a name-conflict 409 names in its envelope.
const STATE_NAME_EXISTS: &str = "name_exists";

/// The most code points a `name` FILTER may carry — the stored cap's number,
/// restated here because the refusal sentence above names it (RULE UFS).
const NAME_FILTER_MAX_CODEPOINTS: usize = 128;

/// The body an empty POST reads as — `req.body() orelse "{}"`, ported.
const EMPTY_OBJECT: &[u8] = b"{}";

/// `GET /v1/tenants/me/workspaces` — one page, oldest first.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/tenants/me/workspaces",
    tag = afd_http::openapi::tag::WORKSPACES,
    operation_id = "list_tenant_workspaces",
    summary = "List the tenant's workspaces",
    description = concat!(
        "Returns a stable oldest-first cursor page of workspaces owned by the ",
        "caller's authoritative tenant. Pass `starting_after` from ",
        "`next_cursor` to continue. The optional `name` filter uses exact ",
        "equality and supports reconciliation after an uncertain workspace- ",
        "create response. ",
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = WorkspacesResponse),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn list<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    RawQuery(query): RawQuery,
) -> Result<Response, Refusal> {
    let person = identity.person();
    let query = query.unwrap_or_default();
    let limit = parse_limit(decoded(&query, "limit")?)?;
    let after = parse_cursor(decoded(&query, "starting_after")?)?;
    let filter = parse_name(decoded(&query, "name")?)?;

    let tenant = tenant_of(&services, person, DETAIL_TENANT_REQUIRED, EVENT_TENANT).await?;

    let page = services
        .workspace_directory()
        .page(&tenant, filter.as_deref(), after.as_ref(), limit)
        .await
        .map_err(Refusal::at(EVENT_LIST))?;
    Ok(Json(page_response(&page, &tenant)).into_response())
}

/// `POST /v1/workspaces` — create one, naming it when the caller did not.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/workspaces",
    tag = afd_http::openapi::tag::WORKSPACES,
    operation_id = "create_workspace",
    summary = "Create a workspace",
    description = concat!(
        "Creates a named workspace in the caller's tenant. The server assigns ",
        "the workspace identifier. This operation does not accept a replay ",
        "key or retry automatically. After an uncertain response, query the ",
        "tenant's workspaces with the exact name. Retrying the same tenant- ",
        "unique name cannot create a second row and returns 409 when the ",
        "first request committed. ",
    ),
    responses(
        (status = 201, description = afd_http::openapi::CREATED, body = CreatedWorkspaceResponse),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn create<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    body: Bytes,
) -> Result<Response, Refusal> {
    let person = identity.person();
    let body = if body.is_empty() { EMPTY_OBJECT } else { &body };
    let request = afd_core::json::object_from_slice::<CreateWorkspaceRequest<'_>>(body)
        .map_err(|_unreadable| Refusal::malformed(DETAIL_CREATE_BODY))?;
    let chosen = match request.name.as_deref() {
        None => None,
        Some(raw) => Chosen::parse(raw).map_err(Refusal::at(EVENT_CREATE))?,
    };

    // Not the shared `tenant_of`: this verb's no-tenant refusal is a 401 with
    // its own sentence, because the remedy is re-authenticating.
    let principal = afd_auth::principal::Principal::Person(person.clone());
    let tenant = match services.workspaces().tenant_of(&principal).await {
        Ok(Some(tenant)) => tenant,
        Ok(None) => return Err(Refusal::unauthorized(DETAIL_CREATE_NO_TENANT)),
        Err(error) => return Err(Refusal::at(EVENT_TENANT)(error)),
    };

    let created = services
        .workspace_directory()
        .create(&tenant, chosen, person.subject().as_str(), services.now())
        .await
        .map_err(|error| {
            if error.code().as_str() == afd_core::error_code::WORKSPACE_NAME_EXISTS.as_str() {
                Refusal::conflict_at(EVENT_CREATE, STATE_NAME_EXISTS)(error)
            } else {
                Refusal::at(EVENT_CREATE)(error)
            }
        })?;
    // Reported after the row is written, so the funnel counts workspaces that
    // exist. Fire-and-forget: the reporter queues and returns, because a person
    // waiting on a 201 must not also be waiting on an analytics endpoint.
    services.analytics().report(&Telemetry::WorkspaceCreated {
        actor: person.subject().as_str().to_owned(),
        workspace_id: created.id.as_str().to_owned(),
        tenant_id: tenant.as_str().to_owned(),
        request_id: RequestId::mint().as_str().to_owned(),
    });

    Ok((
        StatusCode::CREATED,
        Json(created_response(&created, &tenant)),
    )
        .into_response())
}

/// One page, the tenant it belongs to, and the cursor that continues it.
///
/// The cursor is emitted only when a row EXISTS beyond this page — `more` is
/// decided by over-fetching, not by the page being full — so a client never
/// spends a token on a page that comes back empty.
fn page_response<'page>(
    page: &'page WorkspacePage,
    tenant: &'page Uuid7,
) -> WorkspacesResponse<'page> {
    let next_cursor = page.more.then(|| page.rows.last()).flatten().map(|last| {
        Cow::Owned(
            Cursor::Timestamp {
                at_ms: last.created_at_ms,
                id: last.id.clone(),
            }
            .to_string(),
        )
    });
    WorkspacesResponse {
        items: page.rows.iter().map(summary).collect(),
        tenant_id: Cow::Borrowed(tenant.as_str()),
        // Never counted — `tenant_workspaces.zig` answers a literal null.
        total: None,
        next_cursor,
    }
}

/// One row as the wire shows it.
fn summary(row: &WorkspaceRow) -> WorkspaceSummary<'_> {
    WorkspaceSummary {
        id: Cow::Borrowed(&row.id),
        name: row.name.as_deref().map(Cow::Borrowed),
        created_at: row.created_at_ms,
    }
}

/// The create reply, with the identifiers only this side knows.
fn created_response<'created>(
    created: &'created Created,
    tenant: &'created Uuid7,
) -> CreatedWorkspaceResponse<'created> {
    CreatedWorkspaceResponse {
        workspace_id: Cow::Borrowed(created.id.as_str()),
        name: Cow::Borrowed(&created.name),
        request_id: Cow::Owned(RequestId::mint().into()),
        tenant_id: Cow::Borrowed(tenant.as_str()),
    }
}

/// The page size the caller asked for, or the one refusal any wrong spelling
/// earns — `tenant_workspaces.zig` does not say which way a limit was wrong.
fn parse_limit(raw: Option<Cow<'_, str>>) -> Result<u32, Refusal> {
    let Some(raw) = raw else {
        return Ok(LIST_LIMIT_DEFAULT);
    };
    let limit: u32 = raw
        .parse()
        .map_err(|_not_numeric| Refusal::malformed(DETAIL_INVALID_LIMIT))?;
    if limit == 0 || limit > LIST_LIMIT_MAX {
        return Err(Refusal::malformed(DETAIL_INVALID_LIMIT));
    }
    Ok(limit)
}

/// The decoded boundary, or the refusal a foreign token earns.
///
/// The workspace walk's cursor is the `{created_at_ms}:{id}` form with a
/// workspace identifier in its second half — the text-sort form and a
/// non-identifier id are both tokens some OTHER list issued, refused here the
/// way `isSupportedWorkspaceId` refuses them.
fn parse_cursor(raw: Option<Cow<'_, str>>) -> Result<Option<After>, Refusal> {
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
///
/// Bounds only — 1 to 128 code points, no NUL — because a FILTER that would
/// match nothing is the caller's business; the strict character rules belong
/// to the create, where a value is stored rather than compared.
fn parse_name(raw: Option<Cow<'_, str>>) -> Result<Option<String>, Refusal> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let mut codepoints = 0usize;
    for codepoint in raw.chars() {
        if codepoint == '\u{0000}' {
            return Err(Refusal::malformed(DETAIL_INVALID_NAME));
        }
        codepoints += 1;
    }
    if codepoints == 0 || codepoints > NAME_FILTER_MAX_CODEPOINTS {
        return Err(Refusal::malformed(DETAIL_INVALID_NAME));
    }
    Ok(Some(raw.into_owned()))
}

/// One query parameter, percent-decoded — the shared scan, with this route's
/// refusal sentence when a broken escape refuses the whole query string.
fn decoded<'q>(query: &'q str, name: &str) -> Result<Option<Cow<'q, str>>, Refusal> {
    crate::handler::decoded_parameter(query, name)
        .map_err(|_broken| Refusal::malformed(DETAIL_MALFORMED_QUERY))
}
