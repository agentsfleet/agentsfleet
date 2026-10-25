//! One authorized physical send through the same stream reader as fleet execution.

use super::{ProviderRequest, Transport};
use crate::Result;
use crate::model::progress::{FLUSH, Preview};
use afd_crypto::secret::SecretString;
use afd_inference::{
    stream::{StreamTurn, TurnControl},
    transport::{HttpTransport, SendRequest},
};
use afd_wire::inference::ModelInference;
use bytes::Bytes;
use futures_util::future::BoxFuture;
use rig_core::{
    completion::{CompletionRequest, CompletionResponse},
    http_client::{self as rig_http, StreamingResponse},
    streaming::StreamEvent,
};
use std::{sync::Arc, time::Duration};
use tokio::time::{Instant, MissedTickBehavior, interval};

type Completed = (u16, CompletionResponse, Option<crate::types::FailureCause>);

pub(in crate::model) struct Outbound {
    pub(in crate::model) dispatch_id: Box<str>,
    pub(in crate::model) key: SecretString,
    pub(in crate::model) body: CompletionRequest,
    pub(in crate::model) inference: ModelInference,
    pub(in crate::model) model: Box<str>,
    pub(in crate::model) context_tokens: u32,
    pub(in crate::model) timeout: Duration,
    pub(in crate::model) progress: Option<Preview>,
    pub(in crate::model) measurement: Option<afd_inference::InputCount>,
}

pub(in crate::model) async fn complete(
    mut outbound: Outbound,
    physical: Arc<dyn Transport>,
) -> Result<Completed> {
    let configured = afd_inference::Configured::new(&outbound.inference, outbound.context_tokens)?;
    let transport = HttpTransport::new(Arc::new(Once {
        physical,
        timeout: outbound.timeout,
    }));
    let model = configured.model(&outbound.model, outbound.key.expose(), transport.clone());
    let control = TurnControl {
        correlation_id: outbound.dispatch_id.into(),
        deadline: Some(Instant::now() + outbound.timeout),
        ..TurnControl::default()
    };
    let measurement = outbound.measurement.map_or_else(
        || configured.input_count(&outbound.model, &outbound.body),
        Ok,
    )?;
    let mut turn =
        StreamTurn::open_counted(&configured, &model, outbound.body, measurement, control)
            .map_err(|source| {
                crate::Error::from(crate::error::ErrorKind::Sdk { source: *source })
            })?;
    consume(&mut turn, &mut outbound.progress).await;
    let result = turn.finish().await;
    received(&configured, &outbound.model, transport.status(), result).await
}

async fn consume(turn: &mut StreamTurn, progress: &mut Option<Preview>) {
    let mut flush = interval(FLUSH);
    flush.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        let event = tokio::select! {
            event = turn.next() => event,
            _ = flush.tick(), if progress.is_some() => {
                if let Some(progress) = progress { progress.flush(); }
                continue;
            }
        };
        let Some(event) = event else {
            break;
        };
        if let Ok(StreamEvent::Text { text, .. }) = event
            && let Some(progress) = progress
        {
            progress.push(&text);
        }
    }
    if let Some(progress) = progress {
        progress.flush();
    }
}

async fn received(
    configured: &afd_inference::Configured<'_>,
    model: &str,
    status: u16,
    result: afd_inference::stream::TurnResult,
) -> Result<Completed> {
    if let Some(failure) = result.failure {
        if let Some(body) = failure.provider_response_body() {
            if super::rejections::is_authentication_rejection(
                http::StatusCode::from_u16(status).unwrap_or(http::StatusCode::BAD_GATEWAY),
                &serde_json::from_str(body).unwrap_or(serde_json::Value::Null),
            ) {
                return Err(crate::error::ErrorKind::Model.into());
            }
            if let Ok(response) = afd_inference::decode_failed_response(
                configured,
                model,
                http::StatusCode::from_u16(status).unwrap_or(http::StatusCode::BAD_GATEWAY),
                body,
            )
            .await
            {
                return Ok((
                    status,
                    response,
                    Some(crate::types::FailureCause::ProviderInterrupted),
                ));
            }
        }
        // Partial usage may be authoritative even when completion is unverified.
        return Ok((
            http::StatusCode::BAD_GATEWAY.as_u16(),
            result.response,
            result.cause.map(crate::types::FailureCause::from),
        ));
    }
    Ok((status, result.response, None))
}

#[derive(Clone)]
pub(in crate::model) struct Once {
    pub(in crate::model) physical: Arc<dyn Transport>,
    pub(in crate::model) timeout: Duration,
}

impl SendRequest for Once {
    fn send(
        &self,
        request: http::Request<Bytes>,
    ) -> BoxFuture<'static, rig_http::Result<StreamingResponse>> {
        let sender = self.clone();
        Box::pin(async move {
            let request = ProviderRequest {
                request: request.map(|body| body.to_vec()),
                timeout: sender.timeout,
            };
            let response = sender
                .physical
                .send(request)
                .await
                .map_err(rig_http::Error::instance)?;
            let status =
                http::StatusCode::from_u16(response.status).map_err(rig_http::Error::instance)?;
            let mut response_head = http::Response::new(response.body);
            *response_head.status_mut() = status;
            *response_head.headers_mut() = response.headers;
            Ok(response_head)
        })
    }
}
