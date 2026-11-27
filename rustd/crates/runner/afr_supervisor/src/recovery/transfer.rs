//! Exact native presigned transfers carry neither runner tokens nor provider keys.
//! Each object is sealed and opened under the workspace's key the daemon hands over
//! with its authorization, by the cipher the daemon recorded for it.

use crate::{Result, error};
use af_core::id::Uuid7;
use af_crypto::{
    checkpoint::{Aes256Gcm, CheckpointScope, ChunkCipher, Plain},
    secret::SecretBytes,
};
use af_wire::durable_task::{CheckpointCipher, CheckpointObject, CheckpointTransfer};
use base64::Engine as _;
use zeroize::Zeroizing;

/// The largest stored object: a full chunk and the tag the encrypting cipher adds.
const MAX_BYTES: u64 = super::files::CHUNK + af_crypto::TAG_LEN as u64;

/// The implementation the daemon recorded for an object: a save seals with it
/// and a restore opens with it, whatever the deployment's setting is now.
pub(super) fn cipher(named: CheckpointCipher) -> &'static dyn ChunkCipher {
    match named {
        CheckpointCipher::Aes256Gcm => &Aes256Gcm,
        CheckpointCipher::Plain => &Plain,
    }
}

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
        if crate::byte_count(bytes.len()) > MAX_BYTES {
            return Err(error::config("checkpoint upload exceeds its bound"));
        }
        let response = self
            .request(reqwest::Method::PUT, transfer)?
            .body(bytes)
            .send()
            .await
            .map_err(error::checkpoint_transfer("upload"))?;
        if !response.status().is_success() {
            // An uncertain earlier upload is reconciled by native authorization;
            // even a precondition failure is not evidence of matching content.
            return Err(error::config(
                "checkpoint upload requires native reconciliation",
            ));
        }
        Ok(())
    }

    pub(super) async fn download(&self, object: &CheckpointObject) -> Result<Vec<u8>> {
        let expected = stored_length(object)?;
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

/// An object's stored length, when it is one its cipher can have produced
/// from a chunk of at least one byte.
pub(super) fn stored_length(object: &CheckpointObject) -> Result<u64> {
    let smallest = 1 + crate::byte_count(cipher(object.cipher).overhead());
    object
        .ciphertext_length
        .filter(|length| (smallest..=MAX_BYTES).contains(length))
        .ok_or_else(|| error::config("checkpoint ciphertext length is invalid"))
}

pub(super) fn seal(
    workspace: &Uuid7,
    object: &mut CheckpointObject,
    plaintext: Zeroizing<Vec<u8>>,
) -> Result<Vec<u8>> {
    if crate::byte_count(plaintext.len()) != object.plaintext.size
        || af_core::etag::sha256_hex(&plaintext) != object.plaintext.sha256
    {
        return Err(error::config(
            "checkpoint plaintext differs from native registration",
        ));
    }
    let id = Uuid7::parse(&object.object_id)?;
    let (key, nonce) = material(object)?;
    let scope = CheckpointScope {
        workspace_id: workspace,
        repository: &object.repository,
        object_id: &id,
        format_version: 1,
    };
    Ok(cipher(object.cipher).seal(&key, &nonce, &scope.binding(), plaintext)?)
}

pub(super) fn open(
    workspace: &Uuid7,
    object: &mut CheckpointObject,
    ciphertext: Vec<u8>,
) -> Result<SecretBytes> {
    if object.ciphertext_length != Some(crate::byte_count(ciphertext.len()))
        || crate::byte_count(ciphertext.len()) > MAX_BYTES
        || object.ciphertext_sha256.as_deref()
            != Some(af_core::etag::sha256_hex(&ciphertext).as_str())
    {
        return Err(error::config(
            "checkpoint ciphertext differs from native registration",
        ));
    }
    let id = Uuid7::parse(&object.object_id)?;
    let (key, nonce) = material(object)?;
    let scope = CheckpointScope {
        workspace_id: workspace,
        repository: &object.repository,
        object_id: &id,
        format_version: 1,
    };
    let plaintext = cipher(object.cipher).open(&key, &nonce, &scope.binding(), ciphertext)?;
    if crate::byte_count(plaintext.expose().len()) != object.plaintext.size
        || af_core::etag::sha256_hex(plaintext.expose()) != object.plaintext.sha256
    {
        return Err(error::config(
            "checkpoint plaintext authentication mismatch",
        ));
    }
    Ok(plaintext)
}

fn material(object: &mut CheckpointObject) -> Result<(SecretBytes, [u8; af_crypto::NONCE_LEN])> {
    let key = Zeroizing::new(
        object
            .data_key
            .take()
            .ok_or_else(|| error::config("checkpoint key is absent"))?,
    );
    let key = SecretBytes::new(
        base64::prelude::BASE64_STANDARD
            .decode(key.as_bytes())
            .map_err(|source| error::boundary("checkpoint key encoding is invalid", source))?,
    );
    let nonce = base64::prelude::BASE64_STANDARD
        .decode(
            object
                .nonce
                .as_deref()
                .ok_or_else(|| error::config("checkpoint nonce is absent"))?,
        )
        .map_err(|source| error::boundary("checkpoint nonce encoding is invalid", source))?;
    let nonce = nonce
        .as_slice()
        .try_into()
        .map_err(|source| error::boundary("checkpoint nonce length is invalid", source))?;
    Ok((key, nonce))
}

#[cfg(test)]
#[path = "transfer_http/mod.rs"]
pub(super) mod http_tests;
