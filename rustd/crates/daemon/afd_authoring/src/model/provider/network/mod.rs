//! Persisted-model HTTPS with guarded DNS and no automatic retry, redirect or environment proxy.

use super::{ProviderRequest, ProviderResponse, Transport, resolver::Guarded};
use crate::Result;
use futures_util::TryStreamExt as _;
use std::sync::Arc;

/// The production physical provider transport.
#[derive(Debug, Clone)]
pub struct Network {
    client: reqwest::Client,
    #[cfg(feature = "test-util")]
    boundary: Option<(reqwest::Client, reqwest::Url)>,
}

impl Network {
    /// Shares guarded DNS, verified TLS and disabled redirects/retries with native decisions.
    #[must_use]
    pub fn guarded_client(&self) -> reqwest::Client {
        self.client.clone()
    }

    /// Builds the shared guarded client once at daemon boot.
    ///
    /// # Errors
    /// Reports a client/TLS initialization failure.
    pub fn new() -> Result<Self, reqwest::Error> {
        let client = afd_identity::direct(super::super::budget::PROVIDER_TIMEOUT)
            .connect_timeout(af_inference::stream::CONNECT_TIMEOUT)
            .read_timeout(af_inference::stream::READ_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .dns_resolver(Arc::new(Guarded))
            .build()?;
        Ok(Self {
            client,
            #[cfg(feature = "test-util")]
            boundary: None,
        })
    }

    /// Directs provider HTTP to an explicit integration-test boundary.
    /// The guarded client used by decisions and GitHub remains unchanged.
    ///
    /// # Errors
    /// Returns a client initialization failure.
    #[cfg(feature = "test-util")]
    pub fn with_test_endpoint(mut self, endpoint: reqwest::Url) -> Result<Self, reqwest::Error> {
        let client = afd_identity::direct(super::super::budget::PROVIDER_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .build()?;
        self.boundary = Some((client, endpoint));
        Ok(self)
    }
}

#[async_trait::async_trait]
impl Transport for Network {
    async fn send(&self, request: ProviderRequest) -> Result<ProviderResponse> {
        let mut outbound = reqwest::Request::try_from(request.request)?;
        *outbound.timeout_mut() = Some(request.timeout);
        let client = &self.client;
        #[cfg(feature = "test-util")]
        let client = if let Some((client, endpoint)) = &self.boundary {
            *outbound.url_mut() = endpoint.clone();
            client
        } else {
            client
        };
        let response = client.execute(outbound).await?;
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let body = Box::pin(
            response
                .bytes_stream()
                .map_err(rig_core::http_client::Error::instance),
        );
        Ok(ProviderResponse {
            status,
            headers,
            body,
        })
    }
}

#[cfg(test)]
mod tests;
