//! Public upload authorizations must cryptographically bind every physical write restriction.

use super::config::{ACCESS_KEY, SECRET_KEY};
use af_wire::task::CheckpointTransfer;
use aws_credential_types::Credentials;
use aws_sigv4::{
    http_request::{
        PercentEncodingMode, SignableBody, SignableRequest, SignatureLocation, SigningSettings,
        UriPathNormalizationMode, sign,
    },
    sign::v4,
};
use std::{
    collections::BTreeMap,
    time::{Duration, SystemTime},
};

pub(super) fn assert_bound(upload: &CheckpointTransfer) {
    let mut url = reqwest::Url::parse(&upload.url).expect("authorized URL");
    let query: BTreeMap<String, String> = url.query_pairs().into_owned().collect();
    assert_eq!(
        query
            .get("X-Amz-SignedHeaders")
            .expect("required signature parameter"),
        "content-length;host;if-none-match;x-amz-checksum-sha256"
    );
    assert_eq!(
        query
            .get("X-Amz-Algorithm")
            .expect("required signature parameter"),
        "AWS4-HMAC-SHA256"
    );
    assert!(
        query
            .get("X-Amz-Credential")
            .expect("required signature parameter")
            .ends_with("/auto/s3/aws4_request")
    );
    let timestamp = jiff::civil::DateTime::strptime(
        "%Y%m%dT%H%M%SZ",
        query
            .get("X-Amz-Date")
            .expect("required signature parameter"),
    )
    .expect("signature date")
    .to_zoned(jiff::tz::TimeZone::UTC)
    .expect("UTC signature timestamp")
    .timestamp();
    let time: SystemTime = timestamp.into();
    let lifetime = Duration::from_secs(
        query
            .get("X-Amz-Expires")
            .expect("required signature parameter")
            .parse()
            .expect("bounded expiry"),
    );
    assert!(lifetime.as_secs() > 0 && lifetime.as_secs() <= 300);
    url.set_query(None);
    let original = query
        .get("X-Amz-Signature")
        .expect("required signature parameter");
    assert!(
        signature(url.as_str(), &upload.headers, time, lifetime) == *original,
        "official SDK reconstructs the actual native authorization"
    );
    mutations(url.as_str(), upload, time, lifetime, original);
}

fn mutations(
    url: &str,
    upload: &CheckpointTransfer,
    time: SystemTime,
    lifetime: Duration,
    original: &str,
) {
    let length: u64 = upload
        .headers
        .get("content-length")
        .expect("signed physical length")
        .parse()
        .expect("physical length");
    for (name, value) in [
        ("content-length", (length + 1).to_string()),
        ("if-none-match", "different-etag".to_owned()),
        (
            "x-amz-checksum-sha256",
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".to_owned(),
        ),
    ] {
        let mut changed = upload.headers.clone();
        changed.insert(name.to_owned(), value);
        assert!(
            signature(url, &changed, time, lifetime) != original,
            "changing {name} invalidates the issued authorization"
        );
        changed.remove(name);
        assert!(
            signature(url, &changed, time, lifetime) != original,
            "omitting {name} invalidates the issued authorization"
        );
    }
}

fn signature(
    url: &str,
    headers: &BTreeMap<String, String>,
    time: SystemTime,
    lifetime: Duration,
) -> String {
    let identity = Credentials::new(ACCESS_KEY, SECRET_KEY, None, None, "fixture").into();
    let mut settings = SigningSettings::default();
    settings.signature_location = SignatureLocation::QueryParams;
    settings.expires_in = Some(lifetime);
    settings.percent_encoding_mode = PercentEncodingMode::Single;
    settings.uri_path_normalization_mode = UriPathNormalizationMode::Disabled;
    let params = v4::SigningParams::builder()
        .identity(&identity)
        .region("auto")
        .name("s3")
        .time(time)
        .settings(settings)
        .build()
        .expect("complete fixture signing parameters")
        .into();
    let request = SignableRequest::new(
        "PUT",
        url,
        headers
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str())),
        SignableBody::UnsignedPayload,
    )
    .expect("bounded upload request");
    sign(request, &params)
        .expect("official SDK signature")
        .into_parts()
        .1
}
