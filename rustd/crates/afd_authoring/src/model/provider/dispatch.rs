//! The durable dispatch owns one SDK call and one physical send, with no retries.

use super::{ProviderRequest, Transport};
use crate::Result;
use afd_crypto::secret::SecretString;
use afd_wire::{authoring::ENVELOPE_BYTES_MAX, inference::ModelInference};
use rig_core::{
    completion::{CompletionRequest, CompletionResponse},
    driver::{Exchange, Opened, Opening},
    wire::{Encoded, Wire, WireFrame},
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU16, Ordering},
};
use std::time::Duration;

pub(in crate::model) struct Outbound {
    pub(in crate::model) key: SecretString,
    pub(in crate::model) body: CompletionRequest,
    pub(in crate::model) inference: ModelInference,
    pub(in crate::model) model: Box<str>,
    pub(in crate::model) context_tokens: u32,
    pub(in crate::model) timeout: Duration,
}

pub(in crate::model) async fn complete(
    outbound: Outbound,
    physical: Arc<dyn Transport>,
) -> Result<(u16, CompletionResponse)> {
    let configured = afd_inference::Configured::new(&outbound.inference, outbound.context_tokens)?;
    let outcome = Arc::new(Outcome::default());
    let transport = Once {
        physical,
        timeout: outbound.timeout,
        outcome: Arc::clone(&outcome),
    };
    let model = configured.with_transport(&outbound.model, outbound.key.expose(), transport);
    let response = model.call(outbound.body).await;
    if outcome.rejected.load(Ordering::Relaxed) {
        return Err(crate::error::ErrorKind::Model.into());
    }
    Ok((outcome.status.load(Ordering::Relaxed), response?))
}

#[derive(Default)]
struct Outcome {
    status: AtomicU16,
    rejected: AtomicBool,
}

#[derive(Clone)]
struct Once {
    physical: Arc<dyn Transport>,
    timeout: Duration,
    outcome: Arc<Outcome>,
}

impl<W> rig_core::driver::Transport<W> for Once
where
    W: Wire<Payload = Encoded, Frame = WireFrame>,
{
    fn send(&self, payload: Encoded, _exchange: Exchange) -> Opening<WireFrame> {
        let physical = Arc::clone(&self.physical);
        let outcome = Arc::clone(&self.outcome);
        let timeout = self.timeout;
        Opening::new(async move {
            let request = afd_inference::json_request(payload.request)?;
            let path = request.uri().path().to_owned();
            let request = ProviderRequest { request, timeout };
            let reply = physical.send(request).await.map_err(|source| {
                rig_core::ProviderError::from_transport_error(
                    rig_core::http_client::Error::instance(source),
                )
            })?;
            if reply.body.len() > ENVELOPE_BYTES_MAX {
                return Err(rig_core::ProviderError::request(
                    "authoring reply exceeds its byte bound",
                ));
            }
            let code = http::StatusCode::from_u16(reply.status).map_err(|_invalid| {
                rig_core::ProviderError::request("invalid provider HTTP status")
            })?;
            outcome.status.store(reply.status, Ordering::Relaxed);
            // Decode usage even on a failed reply; publication separately checks the HTTP status.
            let document = serde_json::from_slice(&reply.body).map_err(|_invalid| {
                rig_core::ProviderError::request("provider reply is not JSON")
            })?;
            outcome.rejected.store(
                super::rejections::is_authentication_rejection(code, &document),
                Ordering::Relaxed,
            );
            let opened = Opened::new(futures_util::stream::iter([Ok(WireFrame::Bytes(
                reply.body,
            ))]))
            .with_http(code, http::HeaderMap::new())
            .with_route(path)
            .with_document(document);
            Ok(opened)
        })
    }
}
