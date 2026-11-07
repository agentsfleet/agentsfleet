//! Presigned object transfers authenticate transport and enforce exact descriptors.
use super::Transfers;
use afd_wire::task::{
    CheckpointObject, CheckpointObjectStatus, CheckpointPlaintext, CheckpointTransfer,
};
use axum::http::{Method, StatusCode};
use std::time::Duration;
mod server;
use server::Server;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
const CONTENT: &[u8] = b"ciphertext-with-authentication-tag";

fn authority(url: &str) -> CheckpointTransfer {
    CheckpointTransfer {
        url: url.to_owned(),
        headers: vec![
            ("if-none-match".to_owned(), "*".to_owned()),
            ("x-amz-meta-scope".to_owned(), "native-object".to_owned()),
        ]
        .into_iter()
        .collect(),
        expires_at: afd_core::clock::after(afd_core::clock::now(), Duration::from_secs(30)),
    }
}
fn object(url: &str) -> CheckpointObject {
    CheckpointObject {
        object_id: "019f0000-0000-7000-8000-000000000001".to_owned(),
        repository: "owner/repo".to_owned(),
        plaintext: CheckpointPlaintext {
            sha256: "a".repeat(64),
            size: 16,
            kind: "history".to_owned(),
        },
        status: CheckpointObjectStatus::Reused,
        nonce: None,
        data_key: None,
        ciphertext_sha256: Some(afd_core::etag::sha256_hex(CONTENT)),
        ciphertext_length: Some(CONTENT.len() as u64),
        transfer: Some(authority(url)),
    }
}
fn assert_request(seen: &server::Seen, method: &Method, body: &[u8]) {
    assert_eq!(&seen.method, method);
    assert_eq!(seen.path, "/ciphertext?signature=fixture");
    assert_eq!(
        seen.headers
            .get("x-amz-meta-scope")
            .map(axum::http::HeaderValue::as_bytes),
        Some(b"native-object".as_slice())
    );
    assert!(seen.headers.get("authorization").is_none());
    assert!(seen.headers.get("cookie").is_none());
    assert_eq!(seen.bytes.as_ref(), body);
}
#[tokio::test]
async fn verified_https_upload_and_download_preserve_native_ciphertext_authority() -> Result<()> {
    let mut upload = Server::start(StatusCode::OK, Vec::new(), false).await?;
    upload
        .transfers
        .upload(&authority(&upload.url), CONTENT.to_vec())
        .await?;
    let seen = upload.seen.recv().await.ok_or("missing PUT")?;
    assert_request(&seen, &Method::PUT, CONTENT);
    assert_eq!(
        seen.headers
            .get("if-none-match")
            .map(axum::http::HeaderValue::as_bytes),
        Some(b"*".as_slice())
    );
    upload.finish().await?;
    for chunked in [false, true] {
        let mut download = Server::start(StatusCode::OK, CONTENT.to_vec(), chunked).await?;
        assert_eq!(
            download.transfers.download(&object(&download.url)).await?,
            CONTENT
        );
        assert_request(
            &download.seen.recv().await.ok_or("missing GET")?,
            &Method::GET,
            &[],
        );
        download.finish().await?;
    }
    Ok(())
}
#[tokio::test]
async fn upload_failure_and_redirect_require_native_reconciliation_without_retry() -> Result<()> {
    for status in [
        StatusCode::PRECONDITION_FAILED,
        StatusCode::TEMPORARY_REDIRECT,
        StatusCode::SERVICE_UNAVAILABLE,
    ] {
        let mut server = Server::start(status, Vec::new(), false).await?;
        let failure = server
            .transfers
            .upload(&authority(&server.url), CONTENT.to_vec())
            .await
            .err()
            .ok_or("unsafe upload accepted")?;
        assert_eq!(
            failure.code(),
            afd_core::error_code::INTERNAL_OPERATION_FAILED
        );
        assert_request(
            &server.seen.recv().await.ok_or("missing PUT")?,
            &Method::PUT,
            CONTENT,
        );
        server.finish().await?;
    }
    Ok(())
}
#[tokio::test]
async fn downloads_reject_status_length_digest_and_stream_bounds() -> Result<()> {
    let mut changed = CONTENT.to_vec();
    *changed.first_mut().ok_or("nonempty ciphertext")? ^= 1;
    for (status, body, chunked) in [
        (StatusCode::TEMPORARY_REDIRECT, CONTENT.to_vec(), false),
        (StatusCode::OK, vec![0; 17], false),
        (StatusCode::OK, changed, false),
        (StatusCode::OK, vec![0; CONTENT.len() + 1], true),
        (StatusCode::OK, vec![0; 17], true),
    ] {
        let mut server = Server::start(status, body, chunked).await?;
        let failure = server
            .transfers
            .download(&object(&server.url))
            .await
            .err()
            .ok_or("unsafe download accepted")?;
        assert_eq!(
            failure.code(),
            afd_core::error_code::INTERNAL_OPERATION_FAILED
        );
        assert_request(
            &server.seen.recv().await.ok_or("missing GET")?,
            &Method::GET,
            &[],
        );
        server.finish().await?;
    }
    Ok(())
}
#[tokio::test]
async fn absent_or_expired_authority_is_rejected_before_transport() -> Result<()> {
    let transfers = Transfers::new()?;
    let mut transfer = authority("https://checkpoint.invalid/object");
    transfer.headers.clear();
    refused(transfers.upload(&transfer, CONTENT.to_vec()).await)?;
    let mut descriptor = object(&transfer.url);
    descriptor.transfer = None;
    refused(transfers.download(&descriptor).await)?;
    descriptor.transfer = Some(transfer);
    descriptor.ciphertext_length = Some(16);
    refused(transfers.download(&descriptor).await)?;
    for url in [
        "http://checkpoint.invalid/object",
        "https://user:password@checkpoint.invalid/object",
        "https://checkpoint.invalid/object#fragment",
    ] {
        let failure = transfers
            .upload(&authority(url), CONTENT.to_vec())
            .await
            .err()
            .ok_or("invalid authority accepted")?;
        assert_eq!(
            failure.code(),
            afd_core::error_code::INTERNAL_OPERATION_FAILED
        );
    }
    let mut expired = authority("https://checkpoint.invalid/object");
    expired.expires_at = afd_core::clock::now();
    refused(transfers.upload(&expired, CONTENT.to_vec()).await)?;
    Ok(())
}

fn refused<T>(result: crate::Result<T>) -> Result<()> {
    let failure = result.err().ok_or("invalid authority accepted")?;
    assert_eq!(
        failure.code(),
        afd_core::error_code::INTERNAL_OPERATION_FAILED
    );
    Ok(())
}

#[tokio::test]
async fn upload_bounds_and_malformed_signed_headers_fail_before_any_socket() -> Result<()> {
    let transfers = Transfers::new()?;
    let target = "https://checkpoint.invalid/object";
    refused(
        transfers
            .upload(
                &authority(target),
                vec![0; usize::try_from(super::MAX_BYTES)? + 1],
            )
            .await,
    )?;
    for (name, value) in [
        ("invalid header", "value"),
        ("x-native", "value\r\ninjected: true"),
    ] {
        let mut transfer = authority(target);
        transfer.headers.insert(name.to_owned(), value.to_owned());
        refused(transfers.upload(&transfer, CONTENT.to_vec()).await)?;
    }
    Ok(())
}
