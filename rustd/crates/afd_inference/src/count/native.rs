//! Only documented counting endpoints on the same selected provider origin.

use bytes::{Bytes, BytesMut};
use futures_util::TryStreamExt as _;
use rig_core::{completion::CompletionRequest, http_client::HttpClientExt as _, wire::Mode};
use serde_json::{Value, json};
use tokio::time::{Instant, timeout_at};

use crate::{Configured, Result, error::ErrorKind, stream::TurnControl, transport::HttpTransport};
use afd_wire::inference::InferenceApi;

const COUNT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const COUNT_REPLY_BYTES: usize = 16 * 1024;
const COMMON_GENERATION: &[&str] = &["stream", "temperature", "top_p", "metadata"];
const RESPONSES_GENERATION: &[&str] = &[
    "max_output_tokens",
    "stream_options",
    "store",
    "service_tier",
    "background",
    "include",
    "top_logprobs",
    "safety_identifier",
    "prompt_cache_key",
    "prompt_cache_retention",
    "user",
];
const MESSAGES_GENERATION: &[&str] = &["max_tokens", "top_k", "stop_sequences"];

pub(super) fn supported(route: &Configured<'_>) -> bool {
    let settings = &route.settings;
    let base = settings.base_url.trim_end_matches('/');
    match settings.api {
        InferenceApi::Responses => {
            base == "https://api.openai.com/v1"
                && settings
                    .dialect
                    .as_deref()
                    .is_none_or(|value| value == "openai")
        }
        InferenceApi::Messages => {
            base == "https://api.anthropic.com/v1"
                && settings
                    .dialect
                    .as_deref()
                    .is_none_or(|value| value == "anthropic")
        }
        InferenceApi::Gemini => base == "https://generativelanguage.googleapis.com",
        InferenceApi::Chat | InferenceApi::Ollama => false,
    }
}

pub(super) async fn count(
    route: &Configured<'_>,
    model: &str,
    key: &str,
    body: &CompletionRequest,
    transport: &HttpTransport,
    control: &TurnControl,
) -> Result<u64> {
    let request = request(route, model, key, body)?;
    let deadline = (Instant::now() + COUNT_TIMEOUT).min(
        control
            .deadline
            .unwrap_or_else(|| Instant::now() + COUNT_TIMEOUT),
    );
    tokio::select! {
        biased;
        () = control.cancellation.cancelled() => Err(ErrorKind::StreamCancelled.into()),
        counted = timeout_at(deadline, read(transport, request, route.settings.api)) => {
            counted.map_err(|_elapsed| ErrorKind::StreamDeadline)?
        }
    }
}

fn request(
    route: &Configured<'_>,
    model: &str,
    key: &str,
    body: &CompletionRequest,
) -> Result<http::Request<Bytes>> {
    let mut encoded = route.encode_key(model, key, body.clone(), Mode::Streaming)?;
    crate::header_key::move_key(&mut encoded.request)?;
    let encoded = crate::json_request(encoded.request)?;
    let (mut parts, bytes) = encoded.into_parts();
    let mut body: Value = serde_json::from_slice(&bytes)?;
    let path = project(route.settings.api, model, &parts.uri, &mut body)?;
    let mut uri = parts.uri.into_parts();
    uri.path_and_query = Some(path.parse()?);
    parts.uri = http::Uri::from_parts(uri)?;
    parts.headers.remove(http::header::CONTENT_LENGTH);
    parts.headers.remove(http::header::ACCEPT);
    for value in parts.headers.values_mut() {
        value.set_sensitive(true);
    }
    Ok(http::Request::from_parts(
        parts,
        Bytes::from(serde_json::to_vec(&body)?),
    ))
}

fn project(api: InferenceApi, model: &str, uri: &http::Uri, body: &mut Value) -> Result<String> {
    if matches!(api, InferenceApi::Responses | InferenceApi::Messages) {
        remove(body, COMMON_GENERATION)?;
    }
    Ok(match api {
        InferenceApi::Responses => {
            remove(body, RESPONSES_GENERATION)?;
            "/v1/responses/input_tokens".to_owned()
        }
        InferenceApi::Messages => {
            super::messages::validate(body)?;
            remove(body, MESSAGES_GENERATION)?;
            "/v1/messages/count_tokens".to_owned()
        }
        InferenceApi::Gemini => {
            let path = uri
                .path()
                .strip_suffix(":streamGenerateContent")
                .ok_or(ErrorKind::CountReply)?;
            if let Some(object) = body.as_object_mut() {
                let model = model.strip_prefix("models/").unwrap_or(model);
                object.insert("model".into(), json!(format!("models/{model}")));
            }
            *body = json!({"generateContentRequest": body});
            format!("{path}:countTokens")
        }
        InferenceApi::Chat | InferenceApi::Ollama => return Err(ErrorKind::CountReply.into()),
    })
}

fn remove(body: &mut Value, excluded: &[&str]) -> Result<()> {
    let object = body.as_object_mut().ok_or(ErrorKind::CountReply)?;
    for name in excluded {
        object.remove(*name);
    }
    Ok(())
}

async fn read(
    transport: &HttpTransport,
    request: http::Request<Bytes>,
    api: InferenceApi,
) -> Result<u64> {
    let response = transport.send_streaming(request).await?;
    if response.status() != http::StatusCode::OK {
        return Err(ErrorKind::CountStatus {
            status: response.status().as_u16(),
        }
        .into());
    }
    let mut stream = response.into_body();
    let mut bytes = BytesMut::new();
    while let Some(chunk) = stream.try_next().await? {
        if bytes.len().saturating_add(chunk.len()) > COUNT_REPLY_BYTES {
            return Err(ErrorKind::ReplyTooLarge.into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let body: Value =
        serde_json::from_slice(&bytes).map_err(|source| ErrorKind::CountDocument { source })?;
    let field = if api == InferenceApi::Gemini {
        "totalTokens"
    } else {
        "input_tokens"
    };
    body.get(field)
        .and_then(Value::as_u64)
        .filter(|count| *count > 0)
        .ok_or_else(|| ErrorKind::CountReply.into())
}
