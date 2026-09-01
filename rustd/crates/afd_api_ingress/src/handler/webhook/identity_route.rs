//! `POST /v1/auth/identity-events/clerk` — the signup event that opens an account.
//!
//! The port of `http/handlers/auth/identity_events_clerk.zig`. An Svix-signed
//! `user.created` from the identity provider becomes a personal account, and a
//! replay of the same delivery answers exactly as the first did.
//!
//! # Why this is in the ingress plane and not the tenant one
//!
//! Its path sits in the auth family, but nothing about it is bearer-proven:
//! the caller is a vendor, and what it presents is a signature over the body.
//! That is this plane's definition, and the router already splits one family
//! across two planes for the same reason on the connector routes.
//!
//! # A missing secret refuses exactly as every sibling refuses it
//!
//! `identity_events_clerk.zig` answers 500 rather than 401 here, so that an
//! unauthenticated caller cannot learn this deployment configured no secret.
//! This route does NOT follow it, and the departure is deliberate.
//!
//! The status was never the signal: an absent secret and a bad signature both
//! answer 401 across this whole family, and what tells them apart is the code in
//! the body — `UZ-WH-020` against `UZ-WH-010`. Every sibling already carries
//! that distinction, so answering 500 on this one route would not close the leak;
//! it would only mark this route as the special one, which is a louder signal
//! than the code it was meant to hide. If the leak is worth closing it is worth
//! closing for the family, in one change, rather than here.
//!
//! # What is NOT ported, and why the route still serves
//!
//! The Zig branches on `user.deleted` and tears an account down. That is a
//! destructive path with its own blast radius and it is not part of opening an
//! account; porting it under cover of this route would land a delete nobody
//! reviewed. It is answered as an event this daemon serves no rule for — the
//! same 200 every other unhandled type gets — so a provider retries nothing and
//! the gap is visible in the answer rather than hidden in a 404.

use std::sync::Arc;

use afd_core::error_code;
use afd_webhook::vendor::svix::{self, SvixHeaders, SvixSecret};
use afd_webhook::{Refusal as WallRefusal, Verdict};
use axum::extract::State;
use axum::response::{IntoResponse as _, Response};
use axum::{Json, body::Bytes};
use http::{HeaderMap, StatusCode};
use serde::Deserialize;

use crate::handler::Refusal;
use crate::services::{NewAccount, Services, Signups as _, personal_tenant_name};

use super::verify::{header, wall};
use super::{Ignored, within_cap};

/// The scoped event a failed bootstrap is logged under.
const EVENT_BOOTSTRAP: &str = "identity_signup_bootstrap_failed";

/// The event type that opens an account.
const EVENT_USER_CREATED: &str = "user.created";

/// What a caller is told when the body is not an event this route can read.
const DETAIL_UNREADABLE: &str = "The request body is not an identity event";

/// What a caller is told when the event carries no address to open an account
/// under.
///
/// One sentence for both "no primary address" and "an address with no local
/// part", because they are one fault to whoever has to fix it: the provider
/// sent a person this daemon cannot name.
const DETAIL_NO_ADDRESS: &str = "The identity event carries no usable primary email address";

/// The identity provider's `user.created` payload, tolerant of unknown fields.
///
/// Unknown fields are ignored rather than refused, which is the port's rule and
/// not laxity: the provider adds fields to these payloads without notice, and a
/// daemon that refused an unrecognised one would go down on a vendor's release
/// note.
#[derive(Debug, Deserialize)]
struct IdentityEvent {
    /// Which event this is.
    #[serde(rename = "type")]
    kind: String,
    /// The person it is about.
    data: IdentityUser,
}

/// The person an identity event describes.
#[derive(Debug, Deserialize)]
struct IdentityUser {
    /// The provider's own subject, and the account's unique key.
    id: String,
    /// Every address the provider holds for them.
    #[serde(default)]
    email_addresses: Vec<IdentityEmail>,
    /// Which of those is primary.
    #[serde(default)]
    primary_email_address_id: Option<String>,
    #[serde(default)]
    first_name: Option<String>,
    #[serde(default)]
    last_name: Option<String>,
}

/// One address the provider holds.
#[derive(Debug, Deserialize)]
struct IdentityEmail {
    /// Its own id, which `primary_email_address_id` names.
    id: String,
    /// The address itself.
    email_address: String,
}

impl IdentityUser {
    /// The address an account is opened under.
    ///
    /// The one the provider MARKED primary, and only that one. Falling back to
    /// the first address in the list would open an account under whichever
    /// address happened to sort first — a different person's inbox, when a
    /// provider reports several.
    fn primary_email(&self) -> Option<&str> {
        let primary = self.primary_email_address_id.as_deref()?;
        self.email_addresses
            .iter()
            .find(|address| address.id == primary)
            .map(|address| address.email_address.as_str())
    }

    /// What to call them, when the provider said anything at all.
    fn display_name(&self) -> Option<String> {
        let given = self.first_name.as_deref().unwrap_or_default().trim();
        let family = self.last_name.as_deref().unwrap_or_default().trim();
        match (given.is_empty(), family.is_empty()) {
            (true, true) => None,
            (true, false) => Some(family.to_owned()),
            (false, true) => Some(given.to_owned()),
            (false, false) => Some(format!("{given} {family}")),
        }
    }
}

/// `POST /v1/auth/identity-events/clerk`.
///
/// # Errors
/// `UZ-WH-020` for a deployment with no configured secret, `UZ-WH-010` for a
/// signature that did not match, `UZ-WH-011` for one outside its window,
/// `UZ-WH-030` for a body past the cap, and `UZ-REQ-001` for a verified body
/// this route cannot read as an event or that names no usable address.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/auth/identity-events/clerk",
    tag = afd_http::openapi::tag::IDENTITY_EVENTS,
    operation_id = "auth_identity_event_clerk",
    summary = "Receive a Clerk account event",
    description = concat!(
        "Receives signed account events from Clerk. A `user.created` event ",
        "creates the user's account and default workspace. Repeated events ",
        "return `created:false`. Unsupported event types return 200 with ",
        "`status: ignored`. ",
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn receive<D: Services>(
    State(services): State<Arc<D>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, Refusal> {
    within_cap(&body)?;

    // Nothing below this line reads the body as anything but bytes until the
    // signature has been checked over the whole of it.
    let Some(stored) = services.identity_webhook_secret() else {
        return Err(wall(WallRefusal::Unconfigured));
    };
    // A secret that will not parse is `Unconfigured`, never a failed
    // verification — the same reading `verify_svix` makes, for the same reason
    // an absent one is: nothing was checked, so nothing was refused.
    let Some(secret) = std::str::from_utf8(stored.expose())
        .ok()
        .and_then(SvixSecret::parse)
    else {
        return Err(wall(WallRefusal::Unconfigured));
    };

    // Absent headers resolve to empty rather than short-circuiting, because the
    // vendored verifier already refuses an empty field as `Signature`.
    let presented = SvixHeaders {
        id: header(&headers, svix::ID_HEADER).unwrap_or_default(),
        timestamp: header(&headers, svix::TIMESTAMP_HEADER).unwrap_or_default(),
        signature: header(&headers, svix::SIGNATURE_HEADER).unwrap_or_default(),
    };

    match svix::verify_at(&secret, presented, &body, services.now().as_seconds()) {
        Verdict::Verified => {}
        Verdict::Refused(refusal) => return Err(wall(refusal)),
    }

    let event: IdentityEvent = serde_json::from_slice(&body)
        .map_err(|_unreadable| Refusal::coded(error_code::INVALID_REQUEST, DETAIL_UNREADABLE))?;

    if event.kind != EVENT_USER_CREATED {
        // 200 and a reason, never a 4xx — an event this daemon serves no rule
        // for is a correctly-signed delivery, and answering an error would put
        // it in the provider's retry queue forever.
        return Ok((
            StatusCode::OK,
            Json(Ignored {
                ignored: event.kind.as_str().into(),
            }),
        )
            .into_response());
    }

    let Some(email) = event.data.primary_email() else {
        return Err(Refusal::coded(
            error_code::INVALID_REQUEST,
            DETAIL_NO_ADDRESS,
        ));
    };
    // An address with no local part is the same fault as no address at all —
    // see `signup::personal_tenant_name` on why this refuses rather than
    // substituting a name.
    let Some(tenant_name) = personal_tenant_name(email) else {
        return Err(Refusal::coded(
            error_code::INVALID_REQUEST,
            DETAIL_NO_ADDRESS,
        ));
    };

    let display_name = event.data.display_name();
    let opened = services
        .signups()
        .bootstrap(
            NewAccount {
                oidc_subject: &event.data.id,
                email,
                display_name: display_name.as_deref(),
            },
            tenant_name,
            services.now(),
        )
        .await
        .map_err(Refusal::at(EVENT_BOOTSTRAP))?;

    // 200 either way, and the flag says which. A replay is not a conflict: the
    // provider is right to retry and a 409 would put a delivery it cannot
    // change into its retry queue forever.
    Ok((
        StatusCode::OK,
        Json(afd_wire::ingress::AccountOpened {
            workspace_id: opened.workspace_id.into(),
            workspace_name: opened.workspace_name.into(),
            created: opened.created,
        }),
    )
        .into_response())
}
