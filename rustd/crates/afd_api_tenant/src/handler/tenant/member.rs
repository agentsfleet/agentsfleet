//! The people in an account: its owner's members page, and the names a
//! workspace's thread shows beside each sender.

use std::borrow::Cow;
use std::sync::Arc;

use afd_api_wire::team::{MemberSummary, WorkspaceMember};
use afd_core::error_code;
use afd_events::steer_actor;
use afd_http::handler::{IdPath, IdSegment};
use afd_tenant::team::Member;
use axum::Json;
use axum::extract::State;
use axum::response::{IntoResponse as _, Response};
use http::StatusCode;

use crate::auth::WorkspaceContext;
use crate::handler::Refusal;
use crate::services::{Services, TenantTeam as _};

use super::one_page;
use super::own::OwnTenant;

/// The scoped events each verb's failures are logged under.
const EVENT_LIST: &str = "member_list_failed";
/// Pairs with `afd_tenant`'s `workspace_member_removed`.
const EVENT_REMOVE: &str = "member_remove_failed";
const EVENT_NAMES: &str = "workspace_members_failed";

/// The refusal a path segment that is not an identifier earns.
const DETAIL_USER_ID: &str = "user_id must be a valid UUIDv7";

/// The `{user_id}` segment, which a malformed one refuses as [`DETAIL_USER_ID`].
#[derive(Debug)]
pub(crate) struct MemberSegment;

impl IdSegment for MemberSegment {
    const DETAIL: &'static str = DETAIL_USER_ID;
}

/// The state a last-owner 409 names.
const STATE_LAST_OWNER: &str = "last_owner";

/// `GET /v1/tenants/me/members` — the people in the caller's own account.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/tenants/me/members",
    tag = afd_http::openapi::tag::MEMBERS,
    operation_id = "list_members",
    summary = "List the people in your account",
    description = concat!(
        "Returns every member of the caller's own account, oldest membership ",
        "first, with each one's role: `owner` or `member`. One page: ",
        "`next_cursor` is always `null` and `total` counts them all. ",
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = afd_api_wire::tenant::PageResponse<MemberSummary>),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn list<D: Services>(
    State(services): State<Arc<D>>,
    owner: OwnTenant,
) -> Result<Response, Refusal> {
    let members = services
        .team()
        .members(owner.tenant())
        .await
        .map_err(Refusal::at(EVENT_LIST))?;
    Ok(Json(one_page(members.iter().map(member_summary).collect())).into_response())
}

/// `DELETE /v1/tenants/me/members/{user_id}` — remove someone from the account.
#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/v1/tenants/me/members/{user_id}",
    tag = afd_http::openapi::tag::MEMBERS,
    operation_id = "remove_member",
    summary = "Remove a member",
    description = concat!(
        "Removes one person from the caller's own account. Their open streams ",
        "end within 15 seconds and their next request to the account's ",
        "workspaces is refused. Idempotent: somebody who is not a member also ",
        "answers 204. The account's last owner cannot be removed: 409 ",
        "`UZ-INV-004`. ",
    ),
    params(afd_http::openapi::path::Member),
    responses(
        (status = 204, description = afd_http::openapi::NO_CONTENT),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn remove<D: Services>(
    State(services): State<Arc<D>>,
    user: IdPath<MemberSegment>,
    owner: OwnTenant,
) -> Result<Response, Refusal> {
    services
        .team()
        .remove(owner.tenant(), user.id())
        .await
        .map_err(Refusal::conflict_or_at(
            EVENT_REMOVE,
            |error: &afd_tenant::Error| {
                (error.code() == error_code::MEMBER_LAST_OWNER).then_some(STATE_LAST_OWNER)
            },
        ))?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// `GET /v1/workspaces/{workspace_id}/members` — who a thread's senders are.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/workspaces/{workspace_id}/members",
    tag = afd_http::openapi::tag::MEMBERS,
    operation_id = "list_workspace_members",
    summary = "List who can work in a workspace",
    description = concat!(
        "Returns the members of the account that owns the workspace, with ",
        "each one's name and role, so a thread can name who sent each turn. ",
        "Each `actor` equals the `actor` of the messages that member sends. ",
        "No email addresses: every member of the account can read this list. ",
        "One page: `next_cursor` is always `null`. ",
    ),
    params(afd_http::openapi::path::Workspace),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = afd_api_wire::tenant::PageResponse<WorkspaceMember>),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn in_workspace<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
) -> Result<Response, Refusal> {
    let members = services
        .team()
        .members(&owned.tenant)
        .await
        .map_err(Refusal::at(EVENT_NAMES))?;
    Ok(Json(one_page(members.iter().map(workspace_member).collect())).into_response())
}

fn member_summary(member: &Member) -> MemberSummary<'_> {
    MemberSummary {
        user_id: Cow::Borrowed(member.user.as_str()),
        display_name: member.display_name.as_deref().map(Cow::Borrowed),
        email: Cow::Borrowed(&member.email),
        role: Cow::Borrowed(member.role.wire()),
        joined_at: member.joined_at_ms,
    }
}

fn workspace_member(member: &Member) -> WorkspaceMember<'_> {
    WorkspaceMember {
        user_id: Cow::Borrowed(member.user.as_str()),
        display_name: member.display_name.as_deref().map(Cow::Borrowed),
        role: Cow::Borrowed(member.role.wire()),
        actor: Cow::Owned(steer_actor(&member.subject)),
    }
}
