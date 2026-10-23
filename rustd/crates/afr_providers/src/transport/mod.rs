//! Guarded physical sends and explicit runtime retries; the shared inference crate reads replies.

use std::fmt;
use std::sync::Arc;

use afd_core::clock::saturating_millis;
use afd_inference::transport::{HttpTransport, SendRequest};
use afd_wire::policy::CUSTOM_PROVIDER_PREFIX;
use afr_telemetry::labels::{Provider, RetryReason};
use afr_telemetry::record;
use bytes::Bytes;
use futures_util::{TryStreamExt as _, future::BoxFuture};
use reqwest::Url;
use rig_core::http_client::{self as rig_http, Request, StreamingResponse};

use crate::retry::{self, Retrying};

/// The log line a retried send writes.
const EVENT_RETRY: &str = "provider_retry";
/// One lease's transport: the shared client, and whose sends it is making.
#[derive(Clone)]
pub(crate) struct Transport {
    client: reqwest::Client,
    whose: Arc<Whose>,
}

/// Whose sends a transport makes, for its retry line and its retry count.
struct Whose {
    lease_id: Box<str>,
    provider: Box<str>,
    /// The provider as the retry family labels it.
    label: Provider,
}

/// What a log line names for `provider`: a registry name as it is, a `custom:`
/// endpoint by its host alone, so no path, query or userinfo reaches the
/// journal.
fn logged(provider: &str) -> Box<str> {
    provider
        .strip_prefix(CUSTOM_PROVIDER_PREFIX)
        .and_then(|base| Url::parse(base).ok())
        .and_then(|base| {
            base.host_str()
                .map(|host| format!("{CUSTOM_PROVIDER_PREFIX}{host}"))
        })
        .map_or_else(|| provider.into(), String::into_boxed_str)
}

impl fmt::Debug for Transport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Transport")
            .field("lease_id", &self.whose.lease_id)
            .field("provider", &self.whose.provider)
            .finish_non_exhaustive()
    }
}

impl Transport {
    /// Lease `lease_id`'s transport to `provider`, over `client`.
    pub(crate) fn new(client: reqwest::Client, lease_id: &str, provider: &str) -> Self {
        let whose = Whose {
            lease_id: lease_id.into(),
            provider: logged(provider),
            label: Provider::of(provider),
        };
        Self {
            client,
            whose: Arc::new(whose),
        }
    }

    /// Reuses the daemon's SDK HTTP adapter and bounded byte reader.
    pub(crate) fn http(self) -> HttpTransport {
        HttpTransport::new(Arc::new(self))
    }

    /// Sends `parts` with `body` under the bounded retry. Each attempt gets
    /// its own builder, the body shared rather than copied.
    async fn send_retrying(
        &self,
        parts: &http::request::Parts,
        body: &Bytes,
    ) -> reqwest::Result<reqwest::Response> {
        let build = || {
            self.client
                .request(parts.method.clone(), parts.uri.to_string())
                .headers(parts.headers.clone())
                .body(body.clone())
        };
        retry::send(build, |retry| self.retrying(retry)).await
    }

    /// Logs and counts a send about to be retried.
    fn retrying(&self, retry: Retrying) {
        record::retry(self.whose.label, RetryReason::of_status(retry.status));
        let lease_id = &*self.whose.lease_id;
        let provider = &*self.whose.provider;
        let status = retry.status;
        let attempt = retry.attempt;
        let wait_ms = saturating_millis(retry.wait);
        let event = EVENT_RETRY;
        let error_code = afd_core::error_code::INTERNAL_OPERATION_FAILED.as_str();
        tracing::warn!(
            lease_id, provider, status, attempt, wait_ms, event, error_code
        );
    }
}

impl SendRequest for Transport {
    fn send(
        &self,
        request: Request<Bytes>,
    ) -> BoxFuture<'static, rig_http::Result<StreamingResponse>> {
        let transport = self.clone();
        Box::pin(async move {
            let (parts, body) = request.into_parts();
            let response = transport
                .send_retrying(&parts, &body)
                .await
                .map_err(rig_http::Error::instance)?;
            let status = response.status();
            let headers = response.headers().clone();
            let stream = response.bytes_stream().map_err(rig_http::Error::instance);
            let mut response = http::Response::new(Box::pin(stream) as rig_http::BoxedStream);
            *response.status_mut() = status;
            *response.headers_mut() = headers;
            Ok(response)
        })
    }
}

#[cfg(test)]
mod tests;
