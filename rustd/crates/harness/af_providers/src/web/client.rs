//! Pooled guarded HTTP and validation shared by paid web dispatches.

use std::time::Duration;

use af_crypto::secret::SecretString;
use af_wire::web::SearchResult;
use futures_util::TryStreamExt as _;
use reqwest::{Client, Url, header};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::Value;

use super::{RESULT_COUNT, WebFailure};
use crate::{Result, error::raise, transport::REPLY_MAX_BYTES};

// Serde normally accepts a positional array for a struct. Require an object
// for each nested result or error, just as the envelope itself requires one.
#[derive(Deserialize)]
#[serde(transparent, bound(deserialize = "T: Deserialize<'de>"))]
pub(super) struct Object<T>(
    #[serde(deserialize_with = "af_core::json::object_from_deserializer")] pub(super) T,
);

pub(super) fn guarded() -> Result<Client> {
    #[expect(
        clippy::disallowed_methods,
        reason = "the web client uses the shared guarded DNS and redirect policy"
    )]
    af_egress::guarded(Client::builder())
        .retry(reqwest::retry::never())
        .build()
        .map_err(raise::client)
}

pub(super) async fn post<T: DeserializeOwned>(
    client: &Client,
    endpoint: &Url,
    key: &SecretString,
    body: Value,
    timeout: Duration,
) -> Result<T, WebFailure> {
    if key.is_empty() {
        return Err(WebFailure::InvalidCredential);
    }
    let mut credential = header::HeaderValue::from_str(key.expose())
        .map_err(|_invalid_header| WebFailure::InvalidCredential)?;
    credential.set_sensitive(true);
    let response = client
        .post(endpoint.clone())
        .timeout(timeout)
        .header("x-api-key", credential)
        .header(header::CONTENT_TYPE, "application/json")
        .body(body.to_string())
        .send()
        .await
        .map_err(|_transport| WebFailure::Transport)?;
    if !response.status().is_success() {
        return Err(WebFailure::Status {
            status: response.status().as_u16(),
            retry_after: guffbolt_ai::wire::retry_after(response.headers()),
        });
    }
    let bytes = read(response).await?;
    af_core::json::object_from_slice(&bytes)
        .map_err(|_upstream_document| WebFailure::InvalidResponse)
}

async fn read(response: reqwest::Response) -> Result<Vec<u8>, WebFailure> {
    if response
        .content_length()
        .is_some_and(|size| size > u64::try_from(REPLY_MAX_BYTES).unwrap_or(u64::MAX))
    {
        return Err(WebFailure::InvalidResponse);
    }
    let mut bytes = Vec::new();
    let mut body = response.bytes_stream();
    while let Some(chunk) = body
        .try_next()
        .await
        .map_err(|_loss| WebFailure::Transport)?
    {
        if chunk.len() > REPLY_MAX_BYTES - bytes.len() {
            return Err(WebFailure::InvalidResponse);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub(super) fn checked_url(value: &str) -> Result<Url, WebFailure> {
    let url = Url::parse(value).map_err(|_upstream_url| WebFailure::InvalidResponse)?;
    if !matches!(url.scheme(), "https" | "http")
        || !url.has_host()
        || !url.username().is_empty()
        || url.password().is_some()
        || value.chars().any(char::is_control)
    {
        return Err(WebFailure::InvalidResponse);
    }
    Ok(url)
}

pub(super) fn results(
    mut rows: Vec<SearchResult>,
    key: &SecretString,
) -> Result<Vec<SearchResult>, WebFailure> {
    if rows.len() > RESULT_COUNT {
        return Err(WebFailure::InvalidResponse);
    }
    for row in &mut rows {
        let canonical = checked_url(&row.url)?.to_string();
        if std::iter::once(&row.url)
            .chain(std::iter::once(&canonical))
            .chain(&row.title)
            .chain(&row.published)
            .chain(&row.excerpts)
            .any(|text| text.contains(key.expose()))
        {
            return Err(WebFailure::InvalidResponse);
        }
        row.url = canonical;
    }
    Ok(rows)
}
