//! Dispatch already verified deliveries; each route supplies its authenticated replay identity.
use super::{DETAIL_EVENT_HEADER, actor, verify::Verified};
use crate::handler::{Refusal, webhook};
use crate::services::{Services, WebhookIngress as _};
use afd_core::error_code;
use afd_ingress::{Delivery, Surface};
use axum::Json;
use axum::response::{IntoResponse as _, Response};
use http::StatusCode;

pub(super) async fn deliver<D: Services>(
    services: &D,
    proven: Verified,
    event_id: &str,
    event_append: &'static str,
) -> Result<Response, Refusal> {
    if !proven.binding.is_runnable() {
        return Ok((
            StatusCode::OK,
            Json(webhook::Ignored {
                ignored: webhook::REASON_FLEET_PAUSED.into(),
            }),
        )
            .into_response());
    }

    let digest = webhook::json_payload(&proven.body)
        .ok_or_else(|| Refusal::coded(error_code::WEBHOOK_MALFORMED, DETAIL_EVENT_HEADER))?;

    let appended = services
        .ingress()
        .deliver(
            Surface::Fleet,
            &proven.binding,
            &Delivery {
                event_id,
                actor: &actor(proven.binding.source()),
                request_json: &digest,
            },
        )
        .await
        .map_err(Refusal::at(event_append))?;

    Ok((
        StatusCode::ACCEPTED,
        Json(webhook::Accepted {
            event_id: appended.id.as_str().into(),
            replayed: appended.replayed,
        }),
    )
        .into_response())
}
