//! `POST /v1/ingress/{provider}` — one App's deliveries, fanned out.
//!
//! The port of `ingress/github.zig`. Where [`super::github_route`] serves a
//! fleet whose id the URL carries, this serves an INSTALLATION whose fleets
//! have to be looked up: a provider App posts every event for every repository
//! in an organisation to one URL, signed with one secret that belongs to this
//! deployment rather than to any workspace.
//!
//! # Why almost every answer here is a 200
//!
//! An App receives far more than it is asked to act on. Events for repositories
//! nobody subscribed, event kinds no fleet declared, installations connected to
//! no workspace, green builds, label edits — all correctly signed, all real,
//! none of them waking anything. Each answers 200 with the reason, because a
//! 4xx would put a delivery nobody can act on into GitHub's three-day retry
//! loop and change none of them. `UZ-WH-021` and `UZ-WH-022` exist to be
//! REASONS in that shape rather than statuses.
//!
//! The exceptions are the two things a sender can actually fix: a body past the
//! cap, and a signature that did not verify.
//!
//! # What this slice does not carry, and where it went
//!
//! `github.zig` intercepts `deployment_status` and repair-branch traffic before
//! routing, writing repair evidence through `repair_link.zig` and
//! `production_repair_result.zig`. Neither has a Rust home yet — the reader is
//! `afd_runner::sweep::repair`, and the WRITER is unported — so those events
//! fall through to the ordinary classification and are dropped as unsupported.
//! Dropping is the safe direction: the repair sweeper waits rather than acting
//! on evidence it never received. It lands with the repair-evidence port, not
//! here, and until then `deployment_status` is a documented gap rather than a
//! silent one.

use std::sync::Arc;

use afd_core::error_code;
use afd_ingress::{Delivery, Fanout, Surface};
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use axum::{Json, body::Bytes};
use http::{HeaderMap, StatusCode};
use octocrab::models::webhook_events::WebhookEvent;
use serde::Serialize;

use crate::handler::{Refusal, webhook};
use crate::services::{Services, WebhookIngress as _};

use super::github::{Ingest, Policy, classify};

/// The scoped event a failed append is logged under.
const EVENT_APPEND: &str = "app_ingress_append_failed";

/// The scoped event a dropped delivery is logged under.
const EVENT_DROPPED: &str = "app_ingress_dropped";

/// The one provider this daemon serves an App ingress for.
///
/// `webhook_verify.zig` keys its descriptor table by this same word, and
/// [`afd_webhook::Scheme::for_source`] resolves the signature scheme from it.
const PROVIDER_GITHUB: &str = "github";

/// The vault key the GitHub App's own webhook secret is stored under.
///
/// `webhook_verify.zig:54`'s `GITHUB_APP_IDENTITY`, kept byte-for-byte: the
/// secret is stored once by an operator and read by whichever daemon is serving
/// during a cutover, so the key name is a stored-data contract.
const APP_IDENTITY_GITHUB: &str = "github-app";

/// The header GitHub names the delivery's kind in.
const HEADER_EVENT: &str = "x-github-event";

/// The delivery kind an App sends to prove the endpoint answers.
///
/// Answered before any routing: a `ping` carries no repository and belongs to
/// no fleet, and running it through the subscriber lookup would report "nobody
/// subscribed" for a delivery that was never about a subscription.
const EVENT_PING: &str = "ping";

/// The most bytes a delivery may carry.
///
/// `github.zig`'s `MAX_BODY_SIZE`. Checked on the length BEFORE the body is
/// hashed: the cap is what bounds the work one unauthenticated request can ask
/// of this daemon, and spending an HMAC over a body to discover it was too big
/// would spend exactly what the cap exists to protect.
const MAX_BODY_SIZE: usize = 1024 * 1024;

/// The refusal a delivery with no event header earns.
const DETAIL_EVENT_HEADER: &str =
    "Webhook payload could not be parsed. Check Content-Type and body.";

/// The refusal a delivery past the cap earns.
const DETAIL_TOO_LARGE: &str = "The webhook body exceeds the 1 MiB limit. Reduce the payload size.";

/// The refusal a path naming no served provider earns.
const DETAIL_UNKNOWN_PROVIDER: &str = "Unknown App ingress provider";

/// The refusal a delivery matching more fleets than the ceiling earns.
const DETAIL_TOO_MANY: &str = "This delivery matches more fleets than one event may wake.";

/// What a `ping` is answered with.
const STATUS_PONG: &str = "pong";

/// What an App-driven wake records as the actor.
///
/// The App, not the person whose push produced the event and not the fleet's
/// owner: recording either would let an actor-shaped assertion certify that a
/// human woke this fleet when an installation did.
const ACTOR_APP_GITHUB: &str = "github-app";

/// What an accepted App delivery is answered with.
///
/// Wider than [`webhook::Accepted`] because one App delivery is many appends:
/// a sender debugging its integration wants to know how many fleets this
/// installation actually woke, which is the number no single event id can show.
#[derive(Debug, Serialize)]
struct FannedOut {
    /// How many fleets subscribed to this delivery.
    matched: usize,
    /// How many of them this delivery actually appended an event for.
    ///
    /// Lower than `matched` when a fleet already ran this delivery — the claim
    /// is per fleet, so a retry that reaches a wider set than the first attempt
    /// appends only for the fleets that had not seen it.
    enqueued: usize,
}

/// What a `ping` is answered with.
#[derive(Debug, Serialize)]
struct Pong {
    /// Always [`STATUS_PONG`].
    status: &'static str,
}

/// `POST /v1/ingress/{provider}`.
///
/// # Errors
/// `UZ-CONN-004` for a provider this daemon serves no App for, `UZ-WH-030` for
/// a body past the cap, the wall's own refusals, `UZ-WH-002` for a verified
/// body that is not the event its header claims, and `UZ-WH-022` for a delivery
/// matching more fleets than one event may wake.
pub(crate) async fn receive<D: Services>(
    State(services): State<Arc<D>>,
    Path(provider): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, Refusal> {
    if !provider.eq_ignore_ascii_case(PROVIDER_GITHUB) {
        return Err(Refusal::coded(
            error_code::CONNECTOR_UNKNOWN,
            DETAIL_UNKNOWN_PROVIDER,
        ));
    }

    // Before the hash, and so before any work this delivery could have asked
    // for. `Bytes::len` is the buffered length; the router's own body limit is
    // the layer that stops a body from being buffered at all.
    if body.len() > MAX_BODY_SIZE {
        return Err(Refusal::coded(
            error_code::WEBHOOK_PAYLOAD_TOO_LARGE,
            DETAIL_TOO_LARGE,
        ));
    }

    let event = text(&headers, HEADER_EVENT)
        .ok_or_else(|| Refusal::coded(error_code::WEBHOOK_MALFORMED, DETAIL_EVENT_HEADER))?
        .to_owned();

    // Nothing above this line has read the body as anything but bytes.
    let proven = webhook::verified_app(
        &services,
        PROVIDER_GITHUB,
        APP_IDENTITY_GITHUB,
        &headers,
        body,
    )
    .await?;

    // A ping proves the endpoint, and proves it AFTER the signature: an
    // unverified ping answered `pong` would tell a prober the path exists.
    if event == EVENT_PING {
        return Ok((
            StatusCode::OK,
            Json(Pong {
                status: STATUS_PONG,
            }),
        )
            .into_response());
    }

    route(&services, &event, proven.body).await
}

/// A verified delivery, from its payload to its appends.
///
/// Split from [`receive`] so the wall crossing and the routing read as the two
/// separate concerns they are: everything here runs on a body that has already
/// proved itself, and nothing here can reach the body that has not.
async fn route<D: Services>(
    services: &Arc<D>,
    event: &str,
    body: Bytes,
) -> Result<Response, Refusal> {
    // ONE parse, read twice: `octocrab` owns both the routing fields and the
    // classification, so this file walks no JSON of its own.
    let delivery = WebhookEvent::try_from_header_and_body(event, &body).map_err(|_unreadable| {
        Refusal::coded(error_code::WEBHOOK_MALFORMED, DETAIL_EVENT_HEADER)
    })?;

    // Both are the installation's, not a fleet's, and a delivery missing either
    // is one this surface cannot route rather than one it should refuse.
    let (Some(installation), Some(repository)) = (
        delivery.installation.as_ref().map(|install| install.id().0),
        delivery
            .repository
            .as_ref()
            .and_then(|repository| repository.full_name.as_deref()),
    ) else {
        return Ok(dropped(event, webhook::REASON_UNSUPPORTED_EVENT));
    };

    let Some(workspace) = services
        .ingress()
        .installation_workspace(PROVIDER_GITHUB, &installation.to_string())
        .await
        .map_err(Refusal::at(EVENT_DROPPED))?
    else {
        return Ok(dropped(
            event,
            error_code::WEBHOOK_INSTALL_NOT_MAPPED.as_str(),
        ));
    };

    let digest = match classify(Policy::AppIngress, event, &body, services.now())
        .map_err(|_unreadable| Refusal::coded(error_code::WEBHOOK_MALFORMED, DETAIL_EVENT_HEADER))?
    {
        Ingest::Accept(digest) => digest,
        Ingest::Ignore(reason) => return Ok(dropped(event, reason)),
        Ingest::Unsupported => return Ok(dropped(event, webhook::REASON_UNSUPPORTED_EVENT)),
    };

    let subscribers = services
        .ingress()
        .subscribers(&workspace, PROVIDER_GITHUB, repository, event)
        .await
        .map_err(Refusal::at(EVENT_DROPPED))?;

    let fleets = match subscribers {
        Fanout::To(fleets) => fleets,
        Fanout::Nobody => {
            return Ok(dropped(
                event,
                error_code::WEBHOOK_SUBSCRIPTION_NOT_FOUND.as_str(),
            ));
        }
        // Refused rather than truncated. Waking the first hundred of a hundred
        // and one is a silent, order-dependent choice about whose fleet runs,
        // and the operator who wired it that way is the one who has to know.
        Fanout::TooMany(_matched) => {
            return Err(Refusal::coded(
                error_code::WEBHOOK_SUBSCRIPTION_NOT_FOUND,
                DETAIL_TOO_MANY,
            ));
        }
    };

    fan_out(services, &fleets, &digest, &body).await
}

/// Appends the delivery once per subscribed fleet.
///
/// Sequential rather than concurrent: the fan-out is bounded at
/// [`afd_ingress::MAX_FANOUT`] and each append is one round trip, so the
/// latency this saves is not worth a hundred simultaneous claims against one
/// connection pool on a public endpoint.
///
/// A single failed append fails the whole delivery. That is deliberate and it
/// is what makes the retry safe: the claim is per fleet, so the fleets that
/// already appended answer `replayed` on the retry and only the ones that did
/// not are appended again.
async fn fan_out<D: Services>(
    services: &Arc<D>,
    fleets: &[afd_ingress::Binding],
    digest: &str,
    body: &Bytes,
) -> Result<Response, Refusal> {
    let event_id = afd_ingress::replay_id(body);
    let mut enqueued = 0;

    for binding in fleets {
        let appended = services
            .ingress()
            .deliver(
                Surface::App,
                binding,
                &Delivery {
                    event_id: &event_id,
                    actor: ACTOR_APP_GITHUB,
                    request_json: digest,
                },
            )
            .await
            .map_err(Refusal::at(EVENT_APPEND))?;

        if !appended.replayed {
            enqueued += 1;
        }
    }

    Ok((
        StatusCode::ACCEPTED,
        Json(FannedOut {
            matched: fleets.len(),
            enqueued,
        }),
    )
        .into_response())
}

/// The 200 a deliberately-dropped delivery answers with.
///
/// Logged as well as answered, which the per-fleet route does not need to do:
/// there, a drop is visible to whoever configured the URL. Here the sender is
/// an App that will never look, and an operator asking "why did my fleet not
/// run" has only this line to read.
fn dropped(event: &str, reason: &str) -> Response {
    tracing::info!(
        provider = PROVIDER_GITHUB,
        delivery_event = event,
        reason,
        event = EVENT_DROPPED,
    );
    (StatusCode::OK, Json(webhook::Ignored { ignored: reason })).into_response()
}

/// One header's value, when it is one this daemon can read as text.
fn text<'h>(headers: &'h HeaderMap, name: &str) -> Option<&'h str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}
