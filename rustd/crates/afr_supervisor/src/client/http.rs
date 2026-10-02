//! The runner verbs over HTTP, against a real `agentsfleetd`.

use std::time::Duration;

use bytes::Bytes;
use reqwest::StatusCode;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderValue};
use url::Url;

use super::{Call, RunnerApi};
use crate::config::Config;
use crate::error::{self, Result};

/// The media type every runner request body is.
const APPLICATION_JSON: HeaderValue = HeaderValue::from_static("application/json");
/// What precedes the token in `Authorization`.
const BEARER: &str = "Bearer ";
/// How long one call may take, end to end, before it counts as a transport
/// failure. Long enough for a bundle download; renewal races its own deadline
/// rather than relying on this.
const CALL_TIMEOUT: Duration = Duration::from_secs(20);
/// Why a token was refused before any call.
const DETAIL_TOKEN_UNPRINTABLE: &str =
    "AGENTSFLEET_RUNNER_TOKEN holds a byte a header cannot carry";

/// The daemon at `AGENTSFLEET_API_URL`, authenticated by the runner's token.
#[derive(Debug)]
pub(crate) struct HttpRunnerApi {
    client: reqwest::Client,
    base: Url,
    authorization: HeaderValue,
}

impl HttpRunnerApi {
    /// Builds a client for the configured daemon.
    pub(crate) fn new(config: &Config) -> Result<Self> {
        let mut authorization =
            HeaderValue::from_str(&format!("{BEARER}{}", config.token().expose()))
                .map_err(|_unprintable| error::config(DETAIL_TOKEN_UNPRINTABLE))?;
        authorization.set_sensitive(true);
        let client = reqwest::Client::builder()
            .timeout(CALL_TIMEOUT)
            .build()
            .map_err(error::client)?;
        Ok(Self {
            client,
            base: config.api_url().clone(),
            authorization,
        })
    }
}

#[async_trait::async_trait]
impl RunnerApi for HttpRunnerApi {
    async fn send(&self, call: Call) -> Result<Bytes> {
        // Relative, so a base carrying a path prefix keeps it.
        let url = self
            .base
            .join(call.path.trim_start_matches('/'))
            .map_err(error::address)?;
        let request = match call.body {
            Some(body) => self
                .client
                .post(url)
                .header(CONTENT_TYPE, APPLICATION_JSON)
                .body(body),
            None if call.verb.reads() => self.client.get(url),
            None => self.client.post(url),
        };
        let response = request
            .header(AUTHORIZATION, &self.authorization)
            .send()
            .await
            .map_err(error::transport(call.verb))?;
        let status = response.status();
        let body = response
            .bytes()
            .await
            .map_err(error::transport(call.verb))?;
        match status {
            ok if ok.is_success() => Ok(body),
            busy if busy.is_server_error() || busy == StatusCode::TOO_MANY_REQUESTS => {
                Err(error::unavailable(call.verb, busy.as_u16()))
            }
            refused => Err(error::refused_with_body(call.verb, refused.as_u16(), &body)),
        }
    }
}
