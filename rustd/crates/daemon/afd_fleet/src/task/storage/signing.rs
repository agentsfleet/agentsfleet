//! The AWS signer binds physical length, which `object_store` excludes from signatures.

use super::StorageSession;
use crate::Result;
use aws_credential_types::Credentials;
use aws_sigv4::{
    http_request::{
        PercentEncodingMode, SignableBody, SignableRequest, SignatureLocation, SigningSettings,
        UriPathNormalizationMode, sign,
    },
    sign::v4,
};
use object_store::path::Path;
use std::{collections::BTreeMap, time::Duration};

pub(super) async fn upload(
    session: &StorageSession<'_>,
    path: &Path,
    headers: &BTreeMap<String, String>,
    lifetime: Duration,
) -> Result<String> {
    let credentials = session.backend.credentials().get_credential().await?;
    let identity = Credentials::new(
        credentials.key_id.clone(),
        credentials.secret_key.clone(),
        credentials.token.clone(),
        None,
        "checkpoint-vault",
    )
    .into();
    let params = v4::SigningParams::builder()
        .identity(&identity)
        .region("auto")
        .name("s3")
        .time(afd_core::clock::now().into())
        .settings(settings(lifetime))
        .build()?
        .into();
    // Every segment is a validated bucket or a native UUID; no user-selected key exists.
    let url = format!(
        "{}/{}/{}",
        session.store.config.endpoint.trim_end_matches('/'),
        session.store.config.bucket,
        path
    );
    let request = SignableRequest::new(
        "PUT",
        url.as_str(),
        headers
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str())),
        SignableBody::UnsignedPayload,
    )?;
    // The SDK traces private request headers and credential identity at TRACE.
    // Only its synchronous signing call is quiet; native operation spans remain.
    let signed =
        tracing::subscriber::with_default(tracing::subscriber::NoSubscriber::default(), || {
            sign(request, &params)
        })?;
    let (instructions, _) = signed.into_parts();
    let mut request = http::Request::builder()
        .method("PUT")
        .uri(url.as_str())
        .body(())?;
    instructions.apply_to_request_http1x(&mut request);
    Ok(request.uri().to_string())
}

fn settings(lifetime: Duration) -> SigningSettings {
    let mut settings = SigningSettings::default();
    settings.signature_location = SignatureLocation::QueryParams;
    settings.expires_in = Some(lifetime);
    settings.percent_encoding_mode = PercentEncodingMode::Single;
    settings.uri_path_normalization_mode = UriPathNormalizationMode::Disabled;
    settings
}
