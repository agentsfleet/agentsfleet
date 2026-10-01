//! Invites into the caller's own account, and the ones waiting for the caller.
//!
//! An owner issues, lists and revokes invites under `/v1/tenants/me`, which is
//! always the caller's own account, so owning it is a fact of the path. The
//! invitee reads and accepts under `/v1/users/me`, from whichever account sent them.

use std::borrow::Cow;
use std::sync::Arc;

use afd_core::id::Uuid7;
use afd_tenant::error::InviteConflict;
use afd_tenant::team::{Email, Invitee, NewInvite, Waiting};
use afd_wire::team::{AcceptedInviteResponse, CreateInviteRequest, WaitingInvite};
use afd_wire::workspace::WorkspaceAccount;
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use http::StatusCode;

use crate::auth::PersonIdentity;
use crate::handler::Refusal;
use crate::services::{Services, TenantTeam as _, TerminalCredentials as _};

use super::invite_email::email_new_invite;
use super::invite_view::{link_or_refuse, summary};
use super::{DETAIL_TENANT_REQUIRED, one_page, tenant_of};

/// The scoped events each verb's failures are logged under.
const EVENT_CREATE: &str = "invite_create_failed";
const EVENT_LIST: &str = "invite_list_failed";
const EVENT_REVOKE: &str = "invite_revoke_failed";
const EVENT_WAITING: &str = "invite_waiting_failed";
const EVENT_ACCEPT: &str = "invite_accept_failed";
const EVENT_TENANT: &str = "invite_tenant_unresolved";
const EVENT_PERSON: &str = "invite_person_unresolved";

/// The refusal a create body this daemon cannot read earns.
const DETAIL_BODY: &str = "Malformed JSON body";

/// The refusal a path segment that is not an identifier earns.
const DETAIL_INVITE_ID: &str = "invite_id must be a valid UUIDv7";

/// The state a conflicting invite's 409 names.
/// `current_state` on a duplicate invite: the address has a pending invite.
const STATE_INVITED: &str = "invited";
/// `current_state` on a duplicate invite: the address belongs to the account.
const STATE_MEMBER: &str = "member";

/// `POST /v1/tenants/me/invites` — invite an address into the caller's account.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/tenants/me/invites",
    tag = afd_http::openapi::tag::INVITES,
    operation_id = "create_invite",
    summary = "Invite a person into your account",
    description = concat!(
        "Invites one email address into the caller's own account as a member. ",
        "A member opens and steers every workspace in the account and cannot ",
        "store secrets, connect integrations, or manage members. The invite ",
        "expires after 7 days. `link` is the dashboard page the invitee opens ",
        "to accept it. An address that already has a pending invite, or ",
        "already belongs to the account, is refused with 409 `UZ-INV-003`. ",
        "Retrying after an uncertain response is safe: the retry is refused ",
        "with that 409, and the invite list shows the first one. ",
    ),
    request_body = CreateInviteRequest,
    responses(
        (status = 201, description = afd_http::openapi::CREATED, body = afd_wire::team::InviteSummary),
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
        .map_err(|_unreadable| Refusal::malformed(DETAIL_BODY))?;
    let email =
        Email::parse(&request.email, afd_mail::deliverable).map_err(Refusal::at(EVENT_CREATE))?;
    let tenant = tenant_of(&services, person, DETAIL_TENANT_REQUIRED, EVENT_TENANT).await?;
    let inviter = services
        .cli_credentials()
        .user_of(person.subject().as_str())
        .await
        .map_err(Refusal::at(EVENT_PERSON))?;
    let new = NewInvite {
        tenant: &tenant,
        inviter: &inviter.id,
        email: &email,
    };
    let mut invite =
        services
            .team()
            .invite(&new, services.now())
            .await
            .map_err(|error| match error.invite_conflict() {
                Some(InviteConflict::Invited) => {
                    Refusal::conflict_at(EVENT_CREATE, STATE_INVITED)(error)
                }
                Some(InviteConflict::Member) => {
                    Refusal::conflict_at(EVENT_CREATE, STATE_MEMBER)(error)
                }
                None => Refusal::at(EVENT_CREATE)(error),
            })?;
    let link = link_or_refuse(services.dashboard(), &invite.id)?;
    email_new_invite(&*services, &mut invite, person.subject().as_str(), &link).await;
    let summary = summary(services.dashboard(), &invite)?;
    Ok((StatusCode::CREATED, Json(summary)).into_response())
}

/// `GET /v1/tenants/me/invites` — the caller's account's pending invites.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/tenants/me/invites",
    tag = afd_http::openapi::tag::INVITES,
    operation_id = "list_invites",
    summary = "List pending invites",
    description = concat!(
        "Returns every invite into the caller's own account that can still be ",
        "accepted, newest first, each with its accept `link`. An account's ",
        "invites are few, so the list is one page: `next_cursor` is always ",
        "`null` and `total` counts them all. ",
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = afd_wire::tenant::PageResponse<afd_wire::team::InviteSummary>),
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
) -> Result<Response, Refusal> {
    let person = identity.person();
    let tenant = tenant_of(&services, person, DETAIL_TENANT_REQUIRED, EVENT_TENANT).await?;
    let invites = services
        .team()
        .invitations(&tenant, services.now())
        .await
        .map_err(Refusal::at(EVENT_LIST))?;
    let items = invites
        .iter()
        .map(|invite| summary(services.dashboard(), invite))
        .collect::<Result<Vec<_>, _>>()?;
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
        "Revokes one pending invite into the caller's own account, so its link ",
        "stops working. Idempotent: an invite already revoked, already ",
        "accepted, or never this account's also answers 204. ",
    ),
    params(afd_http::openapi::path::Invite),
    responses(
        (status = 204, description = afd_http::openapi::NO_CONTENT),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn revoke<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    Path(invite_id): Path<String>,
) -> Result<Response, Refusal> {
    let person = identity.person();
    let invite = invite_id_of(&invite_id)?;
    let tenant = tenant_of(&services, person, DETAIL_TENANT_REQUIRED, EVENT_TENANT).await?;
    services
        .team()
        .revoke_invitation(&tenant, &invite, services.now())
        .await
        .map_err(Refusal::at(EVENT_REVOKE))?;
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
        "Returns every still-acceptable invite sent to the calling person's ",
        "email address, from any account, newest first. Each item names the ",
        "account it joins. One page: `next_cursor` is always `null`. ",
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = afd_wire::tenant::PageResponse<WaitingInvite>),
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
    let person = identity.person();
    let me = services
        .cli_credentials()
        .user_of(person.subject().as_str())
        .await
        .map_err(Refusal::at(EVENT_PERSON))?;
    let waiting = services
        .team()
        .waiting_for(&me.email, services.now())
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
        "Makes the caller a member of the account the invite opens. An ",
        "operation, in the multi-resource-transaction category: it writes the ",
        "membership and stamps the invite accepted, together. The signed-in ",
        "person's email address must equal the invite's, or it is refused with ",
        "403 `UZ-INV-002`, which does not name the address. An invite that ",
        "expired, was revoked, or was accepted by somebody else answers 404 ",
        "`UZ-INV-001`. Accepting again while still a member answers 200 with ",
        "the account's current workspaces. A member removed since gets 404 ",
        "`UZ-INV-001`. ",
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
    identity: PersonIdentity,
    Path(invite_id): Path<String>,
) -> Result<Response, Refusal> {
    let person = identity.person();
    let invite = invite_id_of(&invite_id)?;
    let me = services
        .cli_credentials()
        .user_of(person.subject().as_str())
        .await
        .map_err(Refusal::at(EVENT_PERSON))?;
    let invitee = Invitee {
        user: &me.id,
        email: &me.email,
    };
    let accepted = services
        .team()
        .accept(&invite, &invitee, services.now())
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

/// The invite a path names, or the refusal a malformed one earns.
pub(super) fn invite_id_of(raw: &str) -> Result<Uuid7, Refusal> {
    Uuid7::parse(raw).map_err(|_unparseable| Refusal::malformed(DETAIL_INVITE_ID))
}

/// One waiting invite, with the account it joins.
fn waiting_invite(waiting: &Waiting) -> WaitingInvite<'_> {
    WaitingInvite {
        id: Cow::Borrowed(&waiting.id),
        account: WorkspaceAccount {
            tenant_id: Cow::Borrowed(&waiting.tenant),
            owner_name: Cow::Borrowed(&waiting.owner_name),
        },
        expires_at: waiting.expires_at_ms,
    }
}
