//! Only documented counting endpoints on the same selected provider origin.

use bytes::{Bytes, BytesMut};
use futures_util::TryStreamExt as _;
use rig_core::{completion::CompletionRequest, http_client::HttpClientExt as _, wire::Mode};
use serde_json::{Value, json};
use tokio::time::{Instant, timeout_at};

use super::super::route::{Api, Route};
use super::super::stream::TurnControl;
use super::super::transport::HttpTransport;
use crate::error::{ErrorKind, Result};

/// How long a native count may take unless the caller's timeouts say
/// otherwise ([`Timeouts::with_count`](super::super::Timeouts::with_count)).
pub(crate) const COUNT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
/// The largest count reply accepted unless the caller's timeouts say
/// otherwise.
pub(crate) const COUNT_REPLY_BYTES: usize = 16 * 1024;
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

/// An API whose provider documents a count endpoint on its own origin.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Native {
    Responses,
    Messages,
    Gemini,
}

/// The count endpoint `route` reaches on its provider's own origin, in that
/// provider's own dialect; `None` for every other route.
pub(super) fn native(route: &Route) -> Option<Native> {
    let settings = route.settings();
    let base = settings.base_url.trim_end_matches('/');
    let dialect = settings.dialect.as_deref();
    match settings.api {
        Api::Responses => (base == "https://api.openai.com/v1"
            && dialect.is_none_or(|value| value == "openai"))
        .then_some(Native::Responses),
        Api::Messages => (base == "https://api.anthropic.com/v1"
            && dialect.is_none_or(|value| value == "anthropic"))
        .then_some(Native::Messages),
        Api::Gemini => {
            (base == "https://generativelanguage.googleapis.com").then_some(Native::Gemini)
        }
        Api::Chat | Api::Ollama => None,
    }
}

pub(super) async fn count(
    native: Native,
    route: &Route,
    model: &str,
    key: &str,
    body: &CompletionRequest,
    transport: &HttpTransport,
    control: &TurnControl,
) -> Result<u64> {
    let request = request(native, route, model, key, body)?;
    let within = control.timeouts.count();
    let deadline =
        (Instant::now() + within).min(control.deadline.unwrap_or_else(|| Instant::now() + within));
    tokio::select! {
        biased;
        () = control.cancellation.cancelled() => Err(ErrorKind::StreamCancelled.into()),
        counted = timeout_at(deadline, read(transport, request, native, control.timeouts.count_reply_bytes())) => {
            counted.map_err(|_elapsed| ErrorKind::StreamDeadline)?
        }
    }
}

fn request(
    native: Native,
    route: &Route,
    model: &str,
    key: &str,
    body: &CompletionRequest,
) -> Result<http::Request<Bytes>> {
    let encoded = route.encode_key(model, key, body.clone(), Mode::Streaming)?;
    let encoded = super::super::json::json_request(encoded.request)?;
    let (mut parts, bytes) = encoded.into_parts();
    let mut body: Value = serde_json::from_slice(&bytes)?;
    let path = project(native, model, &parts.uri, &mut body)?;
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

fn project(native: Native, model: &str, uri: &http::Uri, body: &mut Value) -> Result<String> {
    Ok(match native {
        Native::Responses => {
            remove(body, COMMON_GENERATION)?;
            remove(body, RESPONSES_GENERATION)?;
            "/v1/responses/input_tokens".to_owned()
        }
        Native::Messages => {
            remove(body, COMMON_GENERATION)?;
            super::messages::validate(body)?;
            remove(body, MESSAGES_GENERATION)?;
            "/v1/messages/count_tokens".to_owned()
        }
        Native::Gemini => {
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
    native: Native,
    reply_bytes: usize,
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
        if bytes.len().saturating_add(chunk.len()) > reply_bytes {
            return Err(ErrorKind::ReplyTooLarge.into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let body: Value =
        serde_json::from_slice(&bytes).map_err(|source| ErrorKind::CountDocument { source })?;
    let field = if native == Native::Gemini {
        "totalTokens"
    } else {
        "input_tokens"
    };
    body.get(field)
        .and_then(Value::as_u64)
        .filter(|count| *count > 0)
        .ok_or_else(|| ErrorKind::CountReply.into())
}
