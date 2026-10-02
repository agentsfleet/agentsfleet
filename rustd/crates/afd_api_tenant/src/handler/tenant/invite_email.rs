//! The invite email from the routes' side: count the attempt, send, record
//! what became of it, and report it.
//!
//! The invite is already committed whenever this runs, and nothing here can
//! undo it: a relay that refuses or a store that will not record leaves a valid
//! invite whose status reads `failed`, and the owner can send again or copy the
//! link. Creating an invite runs this once; the send-again route runs it on
//! demand and answers with the outcome.

use std::borrow::Cow;
use std::sync::Arc;

use afd_core::clock::UnixMillis;
use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_core::timing::DAY_MS;
use afd_mail::{INVITE_VALID_DAYS, InviteLetter, InviteSend};
use afd_observability::{InviteEmailOutcome, Telemetry};
use afd_tenant::team::{EmailAttempt, EmailStatus, INVITE_TTL_MS, Invitation};
use afd_wire::team::InviteEmailResponse;
use axum::Json;
use axum::extract::State;
use axum::response::{IntoResponse as _, Response};

use crate::handler::Refusal;
use crate::services::{InviteMail as _, Services, TenantTeam as _};

use super::invite::InvitePath;
use super::invite_view::invite_link;
use super::own::OwnTenant;

// The email tells the invitee how long the link lasts; the store decides it.
// Fail the build, not the reader, when the two drift.
const _: () = assert!(INVITE_VALID_DAYS * DAY_MS == INVITE_TTL_MS);

/// Pairs by verb with `afd_tenant`'s send records; a failed send-again.
const EVENT_SEND: &str = "invite_send_failed";
const EVENT_UNRECORDED: &str = "invite_email_unrecorded";

/// The refusal send-again earns for an invite that cannot be accepted.
const DETAIL_NOT_PENDING: &str = "No pending invite carries that id";

/// The refusal send-again earns when the email did not go.
const DETAIL_UNAVAILABLE: &str = "The invite email could not be sent";

/// What one send came to, as the invite records it.
#[derive(Debug, Clone, Copy)]
pub(super) struct Emailed {
    pub(super) status: EmailStatus,
    pub(super) at: UnixMillis,
}

/// Sends a just-created invite's email and folds the result into `invite`.
///
/// Never fails the create: the invite is committed, so a store that will not
/// count or record the send is logged and the invite reads `failed`.
pub(super) async fn email_new_invite<D: Services>(
    services: &D,
    invite: &mut Invitation,
    actor: &str,
    link: &str,
) {
    let sent = email_invite(services, &invite.tenant, &invite.id, actor, link).await;
    match sent {
        Ok(Some(emailed)) => {
            invite.email_status = emailed.status;
            invite.email_sent_at_ms =
                (emailed.status == EmailStatus::Sent).then(|| emailed.at.as_millis());
        }
        Ok(None) => {}
        Err(error) => unrecorded(&invite.id, &error),
    }
}

/// Counts one more send of `invite`, sends it, records and reports the result.
///
/// `None` when the invite is not `tenant`'s or can no longer be accepted.
pub(super) async fn email_invite<D: Services>(
    services: &D,
    tenant: &Uuid7,
    invite: &Uuid7,
    actor: &str,
    link: &str,
) -> afd_tenant::Result<Option<Emailed>> {
    let Some(attempt) = services
        .team()
        .begin_email(tenant, invite, services.now())
        .await?
    else {
        return Ok(None);
    };
    let outcome = services
        .invite_mail()
        .send(
            services.platform_admin_workspace(),
            &send_of(invite, &attempt, link),
        )
        .await;
    let emailed = Emailed {
        status: status_of(outcome),
        at: services.now(),
    };
    if let Err(error) = services
        .team()
        .record_email(invite, attempt.attempt, emailed.status, emailed.at)
        .await
    {
        unrecorded(invite, &error);
    }
    // Built only for a deployment that reports: the event owns its strings,
    // and a silent sink would copy three of them to discard them.
    let analytics = services.analytics();
    if analytics.is_reporting() {
        analytics.report(&Telemetry::InviteEmail {
            actor: actor.to_owned(),
            tenant_id: tenant.as_str().to_owned(),
            invite_id: invite.as_str().to_owned(),
            attempt: attempt.attempt,
            outcome,
        });
    }
    Ok(Some(emailed))
}

/// What one send hands the mailer: the attempt's addressee and names, and the
/// link that accepts `invite`.
///
/// The names arrive as the store resolved them — an inviter with no display
/// name is named by their address, and the account by its owner's display
/// name or else its own name — and go into the letter unchanged.
fn send_of<'a>(invite: &'a Uuid7, attempt: &'a EmailAttempt, link: &'a str) -> InviteSend<'a> {
    InviteSend {
        invite_id: invite,
        attempt: attempt.attempt,
        to: &attempt.to,
        letter: InviteLetter {
            inviter_name: &attempt.inviter_name,
            owner_name: &attempt.owner_name,
            invite_url: link,
        },
    }
}

/// `POST /v1/tenants/me/invites/{invite_id}/send` — send an invite's email again.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/tenants/me/invites/{invite_id}/send",
    tag = afd_http::openapi::tag::INVITES,
    operation_id = "send_invite_email",
    summary = "Send an invite's email again",
    description = concat!(
        "Sends the invite email for one pending invite into the caller's own ",
        "account, as a new attempt. An operation, in the side-effecting RPC ",
        "category. Each call sends one more email and takes no `Idempotency-Key`. ",
        "Use it when the invite's `email_status` is ",
        "`failed`. When the mail relay is not set up, refuses, or does not ",
        "answer, the route answers 503 `UZ-INV-005`. The invite stays valid ",
        "either way, so its `link` can be shared by hand. An invite that ",
        "expired, was revoked, or was accepted answers 404 `UZ-INV-001`. ",
    ),
    params(afd_http::openapi::path::Invite),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = InviteEmailResponse),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn send<D: Services>(
    State(services): State<Arc<D>>,
    invite: InvitePath,
    owner: OwnTenant,
) -> Result<Response, Refusal> {
    let invite = invite.id();
    let link = invite_link(services.dashboard(), invite);
    let actor = owner.person().subject().as_str();
    let emailed = email_invite(&*services, owner.tenant(), invite, actor, &link)
        .await
        .map_err(Refusal::at(EVENT_SEND))?;
    match emailed.map(|emailed| emailed.status) {
        None => Err(Refusal::coded(
            error_code::INVITE_NOT_FOUND,
            DETAIL_NOT_PENDING,
        )),
        Some(EmailStatus::Sent) => Ok(Json(InviteEmailResponse {
            email_status: Cow::Borrowed(EmailStatus::Sent.wire()),
        })
        .into_response()),
        Some(EmailStatus::Failed | EmailStatus::Unconfigured) => Err(Refusal::coded(
            error_code::INVITE_EMAIL_UNAVAILABLE,
            DETAIL_UNAVAILABLE,
        )),
    }
}

/// The status an outcome is recorded as.
const fn status_of(outcome: InviteEmailOutcome) -> EmailStatus {
    match outcome {
        InviteEmailOutcome::Sent { .. } => EmailStatus::Sent,
        InviteEmailOutcome::Failed { .. } => EmailStatus::Failed,
        InviteEmailOutcome::Unconfigured => EmailStatus::Unconfigured,
    }
}

/// A send whose count or result the store would not take. The invite stands
/// and reads `failed` until an owner sends again.
fn unrecorded(invite: &Uuid7, error: &afd_tenant::Error) {
    let code = error.code().as_str();
    let invite_id = invite.as_str();
    let reason = error.to_string();
    tracing::warn!(
        error_code = code,
        invite_id,
        reason,
        event = EVENT_UNRECORDED
    );
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "test module: an unmet precondition should fail the test loudly"
)]
mod tests {
    use afd_core::id::Uuid7;
    use afd_tenant::team::EmailAttempt;

    use super::send_of;

    const INVITE: &str = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1011";
    const LINK: &str = "https://app.test/invites/0195b4ba-8d3a-7f13-8abc-2b3e1e0c1011";

    /// An inviter who never gave a name: the store names them by address and
    /// the account by its own name, and the letter carries both as resolved,
    /// never swapped and never the invitee's address in either place.
    #[test]
    fn a_nameless_inviters_letter_names_them_by_address_and_the_account_by_its_name() {
        let invite = Uuid7::parse(INVITE).expect("the fixture identifier is UUIDv7");
        let attempt = EmailAttempt {
            attempt: 2,
            to: "carol@example.test".to_owned(),
            inviter_name: "john@example.test".to_owned(),
            owner_name: "john-account".to_owned(),
        };
        let send = send_of(&invite, &attempt, LINK);
        assert_eq!(
            (send.letter.inviter_name, send.letter.owner_name),
            ("john@example.test", "john-account")
        );
        assert_eq!((send.to, send.attempt), ("carol@example.test", 2));
        assert_eq!(send.letter.invite_url, LINK);
        assert_eq!(send.invite_id, &invite);
    }
}
