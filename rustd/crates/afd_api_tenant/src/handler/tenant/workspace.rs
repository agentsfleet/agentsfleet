//! The workspace directory over HTTP: the tenant's list, and the create.
//!
//! The port of `tenant_workspaces.zig` and `workspaces/lifecycle.zig`,
//! sentence for sentence — with one Discovery-logged divergence: a create
//! naming nothing gets a GENERATED name where the Zig daemon answers a 400,
//! because "create me a workspace" was never a naming decision.

use std::sync::Arc;

use afd_tenant::workspace::name::Chosen;
use afd_wire::workspace::CreateWorkspaceRequest;
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

use super::DETAIL_TENANT_REQUIRED;

mod input;
mod render;

pub use self::input::{
    DETAIL_INVALID_CURSOR, DETAIL_INVALID_LIMIT, DETAIL_INVALID_NAME, DETAIL_MALFORMED_QUERY,
};
use self::input::{decoded, parse_cursor, parse_limit, parse_name};
use self::render::{created_response, page_response};

/// The scoped events each verb's failures are logged under.
const EVENT_LIST: &str = "workspace_list_failed";
const EVENT_CREATE: &str = "workspace_create_failed";
const EVENT_TENANT: &str = "workspace_tenant_unresolved";

/// The refusal a create body this daemon cannot read earns.
pub const DETAIL_CREATE_BODY: &str = "Malformed JSON";

/// The create's refusal for a session resolving to no tenant.
///
/// A 401 where the list's is a 403 — `lifecycle.zig`'s split, kept: a list
/// caller lacks a context, a create caller's session has gone stale under it.
pub const DETAIL_CREATE_NO_TENANT: &str = "Missing tenant context on session";

/// The state a name-conflict 409 names in its envelope.
const STATE_NAME_EXISTS: &str = "name_exists";

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
        "Returns a stable oldest-first cursor page of the workspaces in every ",
        "account the caller holds: their own, and each account an accepted ",
        "invite made them a member of. Each item names its account and the ",
        "caller's role there, `owner` or `member`. A tenant API key or a ",
        "command-line credential holds only its own account. `tenant_id` is ",
        "the caller's own account. Pass `starting_after` from ",
        "`next_cursor` to continue. The optional `name` filter uses exact ",
        "equality and supports reconciliation after an uncertain workspace- ",
        "create response. ",
    ),
    params(
        afd_http::openapi::query::WorkspaceFilter,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = afd_wire::workspace::WorkspacesResponse),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
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

    let principal = afd_auth::principal::Principal::Person(person.clone());
    let accounts = services
        .workspace_directory()
        .accounts_of(&principal)
        .await
        .map_err(Refusal::at(EVENT_TENANT))?
        .ok_or_else(|| Refusal::forbidden(DETAIL_TENANT_REQUIRED))?;

    let page = services
        .workspace_directory()
        .page(
            &accounts.tenants(),
            filter.as_deref(),
            after.as_ref(),
            limit,
        )
        .await
        .map_err(Refusal::at(EVENT_LIST))?;
    Ok(Json(page_response(&page, &accounts)?).into_response())
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
        "tenant's workspaces with the exact name. Retrying the same ",
        "tenant-unique name cannot create a second row and returns 409 when the ",
        "first request committed. ",
    ),
    // `Option<…>`, because the body is optional and so is the one field in it:
    // an empty body is read as `{}` and an absent name means "name it for me".
    // The hand-written contract declared this required, which is the defect the
    // generated document exists to stop repeating.
    request_body = Option<CreateWorkspaceRequest>,
    responses(
        (status = 201, description = afd_http::openapi::CREATED, body = afd_wire::workspace::CreatedWorkspaceResponse),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn create<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    body: Bytes,
) -> Result<Response, Refusal> {
    let person = identity.person();
    let body = if body.is_empty() { EMPTY_OBJECT } else { &body };
    let request = afd_http::handler::read_body::<CreateWorkspaceRequest<'_>>(body)
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
