//! Exact native presigned transfers carry neither runner tokens nor provider keys.
//! Each object is sealed and opened under the workspace's key the daemon hands over
//! with its authorization, by the cipher the daemon recorded for it
//! (`af_chunk_store`).

use crate::{Result, error};
use af_chunk_store::MAX_OBJECT_BYTES;
use af_wire::durable_task::{CheckpointObject, CheckpointTransfer};

#[derive(Debug, Clone)]
pub(crate) struct Transfers {
    client: reqwest::Client,
}

impl Transfers {
    pub(crate) fn new() -> Result<Self> {
        #[expect(
            clippy::disallowed_methods,
            reason = "supervisor-only native presigned object URLs; no daemon or tenant credentials, no redirects or physical retries"
        )]
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .timeout(std::time::Duration::from_secs(60))
            .connect_timeout(std::time::Duration::from_secs(5))
            .build()
            .map_err(error::client)?;
        Ok(Self { client })
    }

    pub(super) async fn upload(&self, transfer: &CheckpointTransfer, bytes: Vec<u8>) -> Result<()> {
        if !transfer
            .headers
            .iter()
            .any(|(key, value)| key.eq_ignore_ascii_case("if-none-match") && value == "*")
        {
            return Err(error::config(
                "checkpoint upload lacks create-only authority",
            ));
        }
        if crate::byte_count(bytes.len()) > MAX_OBJECT_BYTES {
            return Err(error::config("checkpoint upload exceeds its bound"));
        }
        let response = self
            .request(reqwest::Method::PUT, transfer)?
            .body(bytes)
            .send()
            .await
            .map_err(error::checkpoint_transfer("upload"))?;
        if !response.status().is_success() {
            // Only an accepted PUT counts: the bucket checked the body against
            // its bound checksum. Even a precondition failure is no evidence
            // of matching content, so the object stays unaccepted, and this
            // save gives way to the next.
            return Err(error::capture_unavailable(
                "checkpoint upload was not accepted",
            ));
        }
        Ok(())
    }

    pub(super) async fn download(&self, object: &CheckpointObject) -> Result<Vec<u8>> {
        let expected = af_chunk_store::stored_length(object)?;
        let transfer = object
            .transfer
            .as_ref()
            .ok_or_else(|| error::config("checkpoint read authority is absent"))?;
        let mut response = self
            .request(reqwest::Method::GET, transfer)?
            .send()
            .await
            .map_err(error::checkpoint_transfer("download"))?;
        if response.status() != reqwest::StatusCode::OK
            || response
                .content_length()
                .is_some_and(|length| length != expected)
        {
            return Err(error::config(
                "checkpoint download does not match its descriptor",
            ));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(error::checkpoint_transfer("download body"))?
        {
            if crate::byte_count(bytes.len()) + crate::byte_count(chunk.len()) > expected {
                return Err(error::config("checkpoint download exceeds its descriptor"));
            }
            bytes.extend_from_slice(&chunk);
        }
        if crate::byte_count(bytes.len()) != expected
            || Some(af_core::etag::sha256_hex(&bytes).as_str())
                != object.ciphertext_sha256.as_deref()
        {
            return Err(error::config("checkpoint ciphertext digest mismatch"));
        }
        Ok(bytes)
    }

    fn request(
        &self,
        method: reqwest::Method,
        transfer: &CheckpointTransfer,
    ) -> Result<reqwest::RequestBuilder> {
        let url = url::Url::parse(&transfer.url)?;
        if url.scheme() != "https"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
            || transfer.expires_at <= af_core::clock::now()
        {
            return Err(error::config(
                "checkpoint transfer authority is invalid or expired",
            ));
        }
        let mut request = self.client.request(method, url).timeout(
            af_core::clock::elapsed(af_core::clock::now(), transfer.expires_at)
                .min(std::time::Duration::from_secs(60)),
        );
        for (name, value) in &transfer.headers {
            let name = reqwest::header::HeaderName::from_bytes(name.as_bytes())
                .map_err(|source| error::boundary("checkpoint signed header is invalid", source))?;
            let mut value = reqwest::header::HeaderValue::from_str(value).map_err(|source| {
                error::boundary("checkpoint signed header value is invalid", source)
            })?;
            value.set_sensitive(true);
            request = request.header(name, value);
        }
        Ok(request)
    }
}

#[cfg(test)]
#[path = "transfer_http/mod.rs"]
pub(super) mod http_tests;
