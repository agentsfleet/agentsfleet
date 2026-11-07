//! Decode a rejected response's received document through the SDK, without a physical send.

use bytes::Bytes;
use rig_core::completion::{CompletionRequest, CompletionResponse};
use rig_core::driver::{Exchange, Opened, Opening};
use rig_core::message::Message;
use rig_core::wire::{Encoded, Wire, WireFrame};

use crate::{Configured, Error, Result};

pub(crate) struct Recovery {
    configured: Configured<'static>,
    model: Box<str>,
}

impl Recovery {
    pub(crate) fn new(configured: &Configured<'_>, model: &str) -> Self {
        Self {
            configured: configured.owned(),
            model: model.into(),
        }
    }

    pub(crate) async fn usage(&self, response: &mut CompletionResponse) {
        if response.raw.is_null()
            || (response.usage.input_tokens.is_some() && response.usage.output_tokens.is_some())
        {
            return;
        }
        if let Ok(recovered) = decode(
            &self.configured,
            &self.model,
            http::StatusCode::OK,
            response.raw.clone(),
        )
        .await
        {
            // Only accounting facts cross this recovery path; the failed answer stays failed.
            let known = &mut response.usage;
            let received = recovered.usage;
            known.input_tokens = known.input_tokens.or(received.input_tokens);
            known.output_tokens = known.output_tokens.or(received.output_tokens);
            known.total_tokens = known.total_tokens.or(received.total_tokens);
            known.cached_input_tokens = known.cached_input_tokens.or(received.cached_input_tokens);
            known.cache_creation_input_tokens = known
                .cache_creation_input_tokens
                .or(received.cache_creation_input_tokens);
            known.tool_use_prompt_tokens = known
                .tool_use_prompt_tokens
                .or(received.tool_use_prompt_tokens);
            known.reasoning_tokens = known.reasoning_tokens.or(received.reasoning_tokens);
            known.cost = known.cost.or(received.cost);
        }
    }
}

/// Retains authoritative usage in an HTTP error's JSON using the configured provider codec.
///
/// # Errors
/// The body is not a provider document; the caller retains its original unknown outcome.
pub async fn decode_failed_response(
    configured: &Configured<'_>,
    model: &str,
    status: http::StatusCode,
    body: &str,
) -> Result<CompletionResponse> {
    let document = serde_json::from_str(body).map_err(Error::from)?;
    decode(configured, model, status, document).await
}

async fn decode(
    configured: &Configured<'_>,
    model: &str,
    status: http::StatusCode,
    document: serde_json::Value,
) -> Result<CompletionResponse> {
    let body = serde_json::to_vec(&document).map_err(Error::from)?;
    let received = Received {
        body: Bytes::from(body),
        document,
        status,
    };
    configured
        .with_transport(model, "", received)
        .call(CompletionRequest::new(Message::user(
            "Decode the received document",
        )))
        .await
        .map_err(Error::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> afd_wire::inference::ModelInference {
        afd_wire::inference::ModelInference {
            api: afd_wire::inference::InferenceApi::Chat,
            base_url: "https://model.example/v1".into(),
            dialect: None,
            max_output_tokens: 37,
        }
    }

    const RECORDED: &str = r#"{"choices":[{"index":0,"message":{"content":"hi"},"finish_reason":null}],"usage":{"prompt_tokens":3,"completion_tokens":2}}"#;

    #[tokio::test]
    async fn an_offline_document_preserves_usage_without_dispatching() -> crate::Result<()> {
        let settings = settings();
        let configured = crate::Configured::new(&settings, 128_000)?;
        let response = super::decode_failed_response(
            &configured,
            "configured-model",
            http::StatusCode::OK,
            RECORDED,
        )
        .await?;
        assert_eq!(response.usage.input_tokens, Some(3));
        Ok(())
    }

    #[tokio::test]
    async fn recovered_counters_preserve_already_reported_cost_and_cache_usage() -> crate::Result<()>
    {
        let settings = settings();
        let configured = Configured::new(&settings, 128_000)?;
        let recovery = Recovery::new(&configured, "configured-model");
        let mut response = decode_failed_response(
            &configured,
            "configured-model",
            http::StatusCode::OK,
            RECORDED,
        )
        .await?;
        response.usage.output_tokens = None;
        response.usage.cached_input_tokens = Some(1);
        response.usage.cache_creation_input_tokens = Some(1);
        response.usage.tool_use_prompt_tokens = Some(1);
        response.usage.reasoning_tokens = Some(1);
        response.usage.cost = Some(rig_core::completion::Cost::from_total(0.01));
        recovery.usage(&mut response).await;
        assert_eq!(response.usage.input_tokens, Some(3));
        assert_eq!(response.usage.output_tokens, Some(2));
        assert_eq!(response.usage.cached_input_tokens, Some(1));
        assert_eq!(response.usage.cache_creation_input_tokens, Some(1));
        assert_eq!(response.usage.tool_use_prompt_tokens, Some(1));
        assert_eq!(response.usage.reasoning_tokens, Some(1));
        assert_eq!(response.usage.cost.map(|cost| cost.total), Some(0.01));
        Ok(())
    }

    #[tokio::test]
    async fn complete_folded_usage_is_not_replaced_by_the_raw_document() -> crate::Result<()> {
        let settings = settings();
        let configured = Configured::new(&settings, 128_000)?;
        let recovery = Recovery::new(&configured, "configured-model");
        let mut response = decode_failed_response(
            &configured,
            "configured-model",
            http::StatusCode::OK,
            RECORDED,
        )
        .await?;
        response.usage.input_tokens = Some(101);
        response.usage.output_tokens = Some(99);
        response.usage.total_tokens = None;
        let usage = response
            .raw
            .get_mut("usage")
            .and_then(serde_json::Value::as_object_mut)
            .ok_or(crate::error::ErrorKind::Body)?;
        usage.insert("total_tokens".into(), serde_json::json!(5));
        recovery.usage(&mut response).await;
        assert_eq!(response.usage.input_tokens, Some(101));
        assert_eq!(response.usage.output_tokens, Some(99));
        assert_eq!(response.usage.total_tokens, None);
        Ok(())
    }
}

#[derive(Clone)]
struct Received {
    body: Bytes,
    document: serde_json::Value,
    status: http::StatusCode,
}

impl<W> rig_core::driver::Transport<W> for Received
where
    W: Wire<Payload = Encoded, Frame = WireFrame>,
{
    fn send(&self, _payload: Encoded, _exchange: Exchange) -> Opening<WireFrame> {
        let received = self.clone();
        Opening::new(async move {
            Ok(Opened::new(futures_util::stream::iter([Ok(WireFrame::Bytes(
                received.body.to_vec(),
            ))]))
            .with_http(received.status, http::HeaderMap::new())
            .with_document(received.document))
        })
    }
}
