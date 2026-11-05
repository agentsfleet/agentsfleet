//! Exact native presigned transfers carry neither runner tokens nor provider keys.

use crate::{Result, error};
use afd_core::id::Uuid7;
use afd_crypto::{
    checkpoint::{CheckpointScope, EncryptedCheckpointObject},
    secret::SecretBytes,
};
use afd_wire::task::{CheckpointObject, CheckpointTransfer};
use base64::Engine as _;
use zeroize::Zeroizing;

const MAX_BYTES: u64 = super::files::CHUNK + afd_crypto::TAG_LEN as u64;

#[derive(Debug)]
pub(super) struct Transfers {
    client: reqwest::Client,
}

impl Transfers {
    pub(super) fn new() -> Result<Self> {
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
        if bytes.len() as u64 > MAX_BYTES {
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
        let expected = object
            .ciphertext_length
            .filter(|length| (17..=MAX_BYTES).contains(length))
            .ok_or_else(|| error::config("checkpoint ciphertext length is invalid"))?;
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
            if bytes.len() as u64 + chunk.len() as u64 > expected {
                return Err(error::config("checkpoint download exceeds its descriptor"));
            }
            bytes.extend_from_slice(&chunk);
        }
        if bytes.len() as u64 != expected
            || Some(afd_core::etag::sha256_hex(&bytes).as_str())
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
            || transfer.expires_at <= afd_core::clock::now()
        {
            return Err(error::config(
                "checkpoint transfer authority is invalid or expired",
            ));
        }
        let mut request = self.client.request(method, url).timeout(
            crate::deadline::left(afd_core::clock::now(), transfer.expires_at)
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

pub(super) fn seal(
    workspace: &Uuid7,
    object: &mut CheckpointObject,
    plaintext: Zeroizing<Vec<u8>>,
) -> Result<Vec<u8>> {
    if plaintext.len() as u64 != object.plaintext.size
        || afd_core::etag::sha256_hex(&plaintext) != object.plaintext.sha256
    {
        return Err(error::config(
            "checkpoint plaintext differs from native registration",
        ));
    }
    let id = Uuid7::parse(&object.object_id)?;
    let (key, nonce) = material(object)?;
    let encrypted = EncryptedCheckpointObject::seal(
        key,
        &nonce,
        CheckpointScope {
            workspace_id: workspace,
            repository: &object.repository,
            object_id: &id,
            format_version: 1,
        },
        plaintext,
    )?;
    let (mut bytes, tag) = encrypted.into_parts();
    bytes.extend_from_slice(&tag);
    Ok(bytes)
}

pub(super) fn open(
    workspace: &Uuid7,
    object: &mut CheckpointObject,
    mut ciphertext: Vec<u8>,
) -> Result<SecretBytes> {
    if object.ciphertext_length != Some(ciphertext.len() as u64)
        || ciphertext.len() as u64 > MAX_BYTES
        || object.ciphertext_sha256.as_deref()
            != Some(afd_core::etag::sha256_hex(&ciphertext).as_str())
    {
        return Err(error::config(
            "checkpoint ciphertext differs from native registration",
        ));
    }
    let tag_at = ciphertext
        .len()
        .checked_sub(afd_crypto::TAG_LEN)
        .ok_or_else(|| error::config("checkpoint authentication tag is absent"))?;
    let tag: [u8; afd_crypto::TAG_LEN] = ciphertext
        .get(tag_at..)
        .ok_or_else(|| error::config("checkpoint authentication tag is absent"))?
        .try_into()
        .map_err(|source| error::boundary("checkpoint authentication tag is invalid", source))?;
    ciphertext.truncate(tag_at);
    let id = Uuid7::parse(&object.object_id)?;
    let (key, nonce) = material(object)?;
    let plaintext = EncryptedCheckpointObject::from_parts(ciphertext, tag).open(
        key,
        &nonce,
        CheckpointScope {
            workspace_id: workspace,
            repository: &object.repository,
            object_id: &id,
            format_version: 1,
        },
    )?;
    if plaintext.expose().len() as u64 != object.plaintext.size
        || afd_core::etag::sha256_hex(plaintext.expose()) != object.plaintext.sha256
    {
        return Err(error::config(
            "checkpoint plaintext authentication mismatch",
        ));
    }
    Ok(plaintext)
}

fn material(object: &mut CheckpointObject) -> Result<(SecretBytes, [u8; afd_crypto::NONCE_LEN])> {
    let key = Zeroizing::new(
        object
            .data_key
            .take()
            .ok_or_else(|| error::config("checkpoint data key is absent"))?,
    );
    let key = SecretBytes::new(
        base64::prelude::BASE64_STANDARD
            .decode(key.as_bytes())
            .map_err(|source| error::boundary("checkpoint data key encoding is invalid", source))?,
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
