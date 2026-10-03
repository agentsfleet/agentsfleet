//! The provider every wire runs on: post one turn, retry it under its bound,
//! and read its stream through the wire's decoder.
//!
//! Generic over the [`Dialect`], so each wire is dispatched statically, and the
//! loop sees one `Box<dyn Provider>` whatever it drives. The key lives in the
//! one request header its wire names and nowhere else: not in a log line, not
//! in `Debug`.

use std::fmt;

use afd_core::clock::saturating_millis;
use bytes::Bytes;
use futures_util::stream::{self, BoxStream};
use futures_util::{StreamExt as _, TryStreamExt as _};
use reqwest::RequestBuilder;
use reqwest::header::CONTENT_TYPE;

use crate::dialect::Dialect;
use crate::error::{Error, Result};
use crate::provider::{Chunk, Provider, Request};
use crate::retry::{self, Retrying};
use crate::sse;

/// Every turn's body is JSON.
const CONTENT_JSON: &str = "application/json";
/// The log line a retried send writes.
const EVENT_RETRY: &str = "provider_retry";

/// The provider credential. `Debug` names it and prints nothing of it.
pub(crate) struct ApiKey(String);

impl ApiKey {
    pub(crate) fn new(key: &str) -> Self {
        Self(key.to_owned())
    }
}

impl fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ApiKey(..)")
    }
}

/// One lease's provider, speaking dialect `D`.
#[derive(Debug)]
pub(crate) struct Http<D> {
    client: reqwest::Client,
    url: String,
    key: ApiKey,
    lease_id: String,
    dialect: D,
}

impl<D: Dialect> Http<D> {
    /// A provider posting turns under `base` with `key`, for lease `lease_id`.
    pub(crate) fn new(
        client: reqwest::Client,
        base: &str,
        key: ApiKey,
        lease_id: &str,
        dialect: D,
    ) -> Self {
        Self {
            client,
            url: format!("{}{}", base.trim_end_matches('/'), D::PATH),
            key,
            lease_id: lease_id.to_owned(),
            dialect,
        }
    }

    /// One send of `body`.
    fn post(&self, body: Bytes) -> RequestBuilder {
        let builder = self
            .client
            .post(self.url.as_str())
            .header(CONTENT_TYPE, CONTENT_JSON)
            .body(body);
        self.dialect.authorize(builder, &self.key.0)
    }

    /// Logs a send about to be retried.
    fn retrying(&self, retry: Retrying) {
        let lease_id = self.lease_id.as_str();
        let provider = D::NAME;
        let status = retry.status;
        let attempt = retry.attempt;
        let wait_ms = saturating_millis(retry.wait);
        let event = EVENT_RETRY;
        tracing::warn!(lease_id, provider, status, attempt, wait_ms, event);
    }
}

impl<D: Dialect> Provider for Http<D> {
    fn stream<'a>(&'a self, request: Request<'a>) -> BoxStream<'a, Result<Chunk>> {
        let opened = async move {
            let body = Bytes::from(self.dialect.body(&request)?);
            let post = || self.post(body.clone());
            let response = retry::send(post, |retry| self.retrying(retry)).await?;
            Ok::<_, Error>(sse::chunks(response, self.dialect.decoder()))
        };
        stream::once(opened).try_flatten().boxed()
    }
}
