//! Schedule observations and webhook addresses are projected from actual shared setup.

use crate::{
    handler::Refusal,
    services::{FleetSchedules as _, Services},
};
use afd_api_wire::fleet::{FleetResponse, WebhookUrl};
use afd_authoring::SavedFleet;
use afd_core::id::Uuid7;
use afd_fleet_runtime::FleetConfig;
use afd_fleet_runtime::config::Trigger;
use afd_webhook::Scheme;
use axum::{
    Json,
    response::{IntoResponse as _, Response},
};
use http::{HeaderValue, StatusCode, header};
use std::borrow::Cow;

const EVENT_SCHEDULES: &str = "fleet_setup_schedules_read_failed";

pub(super) async fn response<D: Services>(
    services: &D,
    workspace: &Uuid7,
    saved: SavedFleet,
    status: StatusCode,
) -> Result<Response, Refusal> {
    let schedules = services
        .schedules()
        .list(workspace, &saved.id)
        .await
        .map_err(Refusal::at(EVENT_SCHEDULES))?;
    let location = format!("/v1/workspaces/{workspace}/fleets/{}", saved.id);
    let etag = saved.etag.clone();
    let webhook_urls = webhook_urls(services.deployment(), &saved.id, saved.definition.runtime());
    let payload = FleetResponse {
        id: saved.id.to_string(),
        name: saved.name,
        status: saved.status,
        etag: saved.etag,
        setup_etag: saved.setup_etag,
        setup_revision: saved.setup_revision,
        policy_semantics: saved.policy_semantics,
        definition: saved.definition,
        setup: saved.setup,
        start: saved.start,
        schedules: crate::handler::schedule::views(&schedules, services.now()),
        webhook_urls,
        created_at: saved.created_at,
        updated_at: saved.updated_at,
        events_processed: saved.events_processed,
        budget_used_nanos: saved.budget_used_nanos,
        pending_approvals: saved.pending_approvals,
        memory_access: afd_api_wire::fleet::MemoryAccess {
            read: saved.memory_reads_workspace,
            publish: saved.memory_publishes_workspace,
        },
    };
    let mut response = (status, Json(payload)).into_response();
    attach(&mut response, header::ETAG, &etag);
    if status == StatusCode::CREATED {
        attach(&mut response, header::LOCATION, &location);
    }
    Ok(response)
}

/// Where each declared webhook trigger is delivered, on this deployment.
///
/// A source whose signature scheme the ingress ships is served at
/// `/v1/webhooks/{fleet_id}/{source}` — `github` by its own literal route,
/// every other by the per-source route (`afd_http::route::webhook`). The
/// source needs no escaping: the parser admits only a lowercase slug, and
/// never `approval`. Any other source parsed only because it declared a Svix
/// secret, so its one verifying route is `/v1/webhooks/svix/{fleet_id}`; the
/// parser keeps that source the fleet's first webhook trigger, the one that
/// route reaches.
fn webhook_urls(deployment: &str, fleet: &Uuid7, config: &FleetConfig) -> Vec<WebhookUrl<'static>> {
    config
        .triggers()
        .iter()
        .filter_map(|trigger| match trigger {
            Trigger::Webhook(webhook) => Some(WebhookUrl {
                source: Cow::Owned(webhook.source.to_string()),
                url: Cow::Owned(match Scheme::for_source(&webhook.source) {
                    Some(_) => format!("{deployment}/v1/webhooks/{fleet}/{}", webhook.source),
                    None => format!("{deployment}/v1/webhooks/svix/{fleet}"),
                }),
            }),
            _ => None,
        })
        .collect()
}

/// Sets `name` when `value` is a legal header value; a value that is not is
/// left off rather than failing a response that already succeeded.
pub(super) fn attach(response: &mut Response, name: header::HeaderName, value: &str) {
    if let Ok(value) = HeaderValue::from_str(value) {
        response.headers_mut().insert(name, value);
    }
}

#[cfg(test)]
mod tests;
