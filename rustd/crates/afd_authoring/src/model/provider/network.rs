//! Persisted-model HTTPS with guarded DNS and no automatic retry, redirect or environment proxy.

use super::{ProviderRequest, ProviderResponse, Transport, resolver::Guarded};
use crate::{Result, error::ErrorKind};
use afd_wire::authoring::ENVELOPE_BYTES_MAX;
use std::{sync::Arc, time::Duration};

/// The production physical provider transport.
#[derive(Debug, Clone)]
pub struct Network {
    client: reqwest::Client,
}

impl Network {
    /// Builds the shared guarded client once at daemon boot.
    ///
    /// # Errors
    /// Reports a client/TLS initialization failure.
    pub fn new() -> Result<Self, reqwest::Error> {
        let client = afd_identity::direct(Duration::from_millis(
            super::super::budget::PROVIDER_TIMEOUT_MS.cast_unsigned(),
        ))
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .dns_resolver(Arc::new(Guarded))
        .build()?;
        Ok(Self { client })
    }
}

#[async_trait::async_trait]
impl Transport for Network {
    async fn send(&self, request: ProviderRequest) -> Result<ProviderResponse> {
        let mut outbound = reqwest::Request::try_from(request.request)?;
        *outbound.timeout_mut() = Some(Duration::from_millis(request.timeout_ms));
        let mut response = self.client.execute(outbound).await?;
        let status = response.status().as_u16();
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if body.len().saturating_add(chunk.len()) > ENVELOPE_BYTES_MAX {
                return Err(ErrorKind::Unknown.into());
            }
            body.extend_from_slice(&chunk);
        }
        Ok(ProviderResponse { status, body })
    }
}
