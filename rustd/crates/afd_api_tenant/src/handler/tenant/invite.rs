//! Invites into the caller's own account, and the ones waiting for the caller.
//!
//! An owner issues, lists and revokes invites under `/v1/tenants/me`, which is
//! always the caller's own account, so owning it is a fact of the path. The
//! invitee reads and accepts under `/v1/users/me`, from whichever account sent them.

use std::borrow::Cow;
use std::sync::Arc;

use afd_api_wire::team::{AcceptedInviteResponse, CreateInviteRequest, WaitingInvite};
use afd_api_wire::workspace::WorkspaceAccount;
use afd_auth::principal::Person;
use afd_http::handler::{IdPath, IdSegment};
use afd_tenant::cli_credential::UserIdentity;
use afd_tenant::error::InviteConflict;
use afd_tenant::team::{Email, Invitee, NewInvite, Waiting};
use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::response::{IntoResponse as _, Response};
use http::StatusCode;

use crate::auth::PersonIdentity;
use crate::handler::Refusal;
use crate::services::{Services, TenantTeam as _, TerminalCredentials as _};

use super::invite_email::email_new_invite;
use super::invite_view::{invite_link, summary};
use super::own::{OwnTenant, own_tenant};
use super::{DETAIL_MALFORMED_BODY, one_page};

// The scoped events each verb's failures are logged under. A write's failure
// pairs by name with `afd_tenant`'s success record for the same verb, named
// beside it; the reads have no success record to pair with.
/// Pairs with `workspace_invite_created`.
const EVENT_CREATE: &str = "invite_create_failed";
const EVENT_LIST: &str = "invite_list_failed";
/// Pairs with `workspace_invite_revoked`.
const EVENT_REVOKE: &str = "invite_revoke_failed";
const EVENT_WAITING: &str = "invite_waiting_failed";
/// Pairs with `workspace_invite_accepted`.
const EVENT_ACCEPT: &str = "invite_accept_failed";
const EVENT_PERSON: &str = "invite_person_unresolved";

/// The refusal a path segment that is not an identifier earns.
const DETAIL_INVITE_ID: &str = "invite_id must be a valid UUIDv7";

/// The `{invite_id}` segment, which a malformed one refuses as
/// [`DETAIL_INVITE_ID`].
#[derive(Debug)]
pub(crate) struct InviteSegment;

impl IdSegment for InviteSegment {
    const DETAIL: &'static str = DETAIL_INVITE_ID;
}

/// The invite a route's path names, parsed before its handler runs.
pub(super) type InvitePath = IdPath<InviteSegment>;

/// The state a conflicting invite's 409 names.
/// `current_state` on a duplicate invite: the address has a pending invite.
const STATE_INVITED: &str = "invited";
/// `current_state` when the address belongs to the account: a duplicate
/// invite, or a revoke of one its invitee already joined through.
const STATE_MEMBER: &str = "member";

/// `POST /v1/tenants/me/invites` — invite an address into the caller's account.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/tenants/me/invites",
    tag = afd_http::openapi::tag::INVITES,
    operation_id = "create_invite",
    summary = "Invite a person into your account",
    description = concat!(
        "Invites an email address to your account for seven days. Members can use every ",
        "workspace but cannot store secrets, connect apps, or manage members. Share the ",
        "returned `link`.\n\n",
        "Existing invites or members return 409 `UZ-INV-003`, with `current_state` of ",
        "`invited` or `member`. After an uncertain response, check the invite list; ",
        "retrying cannot create another pending invite.",
    ),
    request_body = CreateInviteRequest,
    responses(
        (status = 201, description = afd_http::openapi::CREATED, body = afd_api_wire::team::InviteSummary),
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
    let request = afd_http::handler::read_body::<CreateInviteRequest<'_>>(&body)
        .map_err(|_unreadable| Refusal::malformed(DETAIL_MALFORMED_BODY))?;
    let email =
        Email::parse(&request.email, afd_mail::deliverable).map_err(Refusal::at(EVENT_CREATE))?;
    // Two reads that need nothing from each other, so neither waits on the other.
    let (tenant, inviter) =
        tokio::try_join!(own_tenant(&services, person), me(&*services, person))?;
    let new = NewInvite {
        tenant: &tenant,
        inviter: &inviter.id,
        email: &email,
    };
    let mut invite = services
        .team()
        .invite(&new, services.now())
        .await
        .map_err(refusal_at(EVENT_CREATE))?;
    let link = invite_link(services.dashboard(), &invite.id);
    email_new_invite(&*services, &mut invite, person.subject().as_str(), &link).await;
    Ok((StatusCode::CREATED, Json(summary(&invite, link))).into_response())
}

/// `GET /v1/tenants/me/invites` — the caller's account's pending invites.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/tenants/me/invites",
    tag = afd_http::openapi::tag::INVITES,
    operation_id = "list_invites",
    summary = "List pending invites",
    description = concat!(
        "Lists pending account invites newest first, including acceptance links. Results ",
        "use one page: `next_cursor` is null and `total` counts all invites.",
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = afd_api_wire::tenant::PageResponse<afd_api_wire::team::InviteSummary>),
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
    let invites = services
        .team()
        .invitations(owner.tenant())
        .await
        .map_err(Refusal::at(EVENT_LIST))?;
    let dashboard = services.dashboard();
    let items = invites
        .iter()
        .map(|invite| summary(invite, invite_link(dashboard, &invite.id)))
        .collect();
    Ok(Json(one_page(items)).into_response())
}

/// `DELETE /v1/tenants/me/invites/{invite_id}` — revoke a pending invite.
#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/v1/tenants/me/invites/{invite_id}",
    tag = afd_http::openapi::tag::INVITES,
    operation_id = "revoke_invite",
    summary = "Revoke an invite",
    description = concat!(
        "Revokes a pending invite and disables its link. Missing, already revoked, or ",
        "accepted invites for former members return 204. An accepted invite for a current ",
        "member returns 409 `UZ-INV-003` with `member`; remove that member instead.",
    ),
    params(afd_http::openapi::path::Invite),
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
pub(crate) async fn revoke<D: Services>(
    State(services): State<Arc<D>>,
    invite: InvitePath,
    owner: OwnTenant,
) -> Result<Response, Refusal> {
    services
        .team()
        .revoke_invitation(owner.tenant(), invite.id())
        .await
        .map_err(refusal_at(EVENT_REVOKE))?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// `GET /v1/users/me/invites` — the invites waiting for the caller's address.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/users/me/invites",
    tag = afd_http::openapi::tag::INVITES,
    operation_id = "list_my_invites",
    summary = "List invites waiting for you",
    description = concat!(
        "Lists pending invites sent to your email address across all accounts, newest ",
        "first. Each invite names its account. Results use one page; `next_cursor` is null.",
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = afd_api_wire::tenant::PageResponse<WaitingInvite>),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::UNKNOWN_SUBJECT),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn waiting<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
) -> Result<Response, Refusal> {
    let me = me(&*services, identity.person()).await?;
    let waiting = services
        .team()
        .waiting_for(&me.email)
        .await
        .map_err(Refusal::at(EVENT_WAITING))?;
    Ok(Json(one_page(waiting.iter().map(waiting_invite).collect())).into_response())
}

/// `POST /v1/users/me/invites/{invite_id}/accept` — join the account an invite opens.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/users/me/invites/{invite_id}/accept",
    tag = afd_http::openapi::tag::INVITES,
    operation_id = "accept_invite",
    summary = "Accept an invite",
    description = concat!(
        "Joins the invited account. Your signed-in email must match the invite, or the ",
        "request returns 403 `UZ-INV-002`. Expired, revoked, or otherwise unavailable ",
        "invites return 404 `UZ-INV-001`.\n\n",
        "Accepting again while still a member returns 200 with current workspaces. After ",
        "removal from the account, the old invite returns 404.",
    ),
    params(afd_http::openapi::path::Invite),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = AcceptedInviteResponse),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn accept<D: Services>(
    State(services): State<Arc<D>>,
    invite: InvitePath,
    identity: PersonIdentity,
) -> Result<Response, Refusal> {
    let me = me(&*services, identity.person()).await?;
    let invitee = Invitee {
        user: &me.id,
        email: &me.email,
    };
    let accepted = services
        .team()
        .accept(invite.id(), &invitee, services.now())
        .await
        .map_err(Refusal::at(EVENT_ACCEPT))?;
    Ok(Json(AcceptedInviteResponse {
        tenant_id: Cow::Borrowed(accepted.tenant.as_str()),
        workspace_ids: accepted
            .workspaces
            .iter()
            .map(|id| Cow::Borrowed(id.as_str()))
            .collect(),
    })
    .into_response())
}

/// The user row behind `person`, or the refusal a subject with none earns.
async fn me<D: Services>(services: &D, person: &Person) -> Result<UserIdentity, Refusal> {
    services
        .cli_credentials()
        .user_of(person.subject().as_str())
        .await
        .map_err(Refusal::at(EVENT_PERSON))
}

/// Renders an invite store failure as `event`'s refusal, naming the state a
/// conflict carries so a client branches on it without re-reading the invite.
fn refusal_at(event: &'static str) -> impl FnOnce(afd_tenant::Error) -> Refusal {
    Refusal::conflict_or_at(event, |error: &afd_tenant::Error| {
        error.invite_conflict().map(|conflict| match conflict {
            InviteConflict::Invited => STATE_INVITED,
            InviteConflict::Member => STATE_MEMBER,
        })
    })
}

/// One waiting invite, with the account it joins.
fn waiting_invite(waiting: &Waiting) -> WaitingInvite<'_> {
    WaitingInvite {
        id: Cow::Borrowed(waiting.id.as_str()),
        account: WorkspaceAccount {
            tenant_id: Cow::Borrowed(waiting.tenant.as_str()),
            owner_name: Cow::Borrowed(&waiting.owner_name),
        },
        expires_at: waiting.expires_at,
    }
}
