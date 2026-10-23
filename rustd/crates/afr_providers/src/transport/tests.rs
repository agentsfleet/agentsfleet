#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]
#![expect(
    clippy::disallowed_methods,
    reason = "the transport under test wraps a plain loopback client; the egress guard and afd_identity::direct are not what these tests prove"
)]

use afd_inference::transport::REPLY_MAX_BYTES;
use std::convert::Infallible;
use std::error::Error as StdError;

use bytes::Bytes;
use futures_util::stream;
use http::Method;
use rig_core::http_client::{self as rig_http, HttpClientExt as _, MultipartForm};

use super::{Transport, logged};

/// The bytes one chunk of a served reply carries.
const SERVED_CHUNK: usize = 64 * 1024;

/// A server on a loopback port answering every request with `chunks` body
/// chunks of [`SERVED_CHUNK`] bytes each; where a turn posts to it.
async fn serve_chunks(chunks: usize) -> String {
    let app = axum::Router::new().fallback(move || async move {
        let chunk = Bytes::from(vec![b'x'; SERVED_CHUNK]);
        let body = std::iter::repeat_n(chunk, chunks).map(Ok::<_, Infallible>);
        axum::body::Body::from_stream(stream::iter(body))
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{address}/v1/chat/completions")
}

/// A reply of `chunks` served chunks, answered through rig's buffered seam,
/// its body still to be read.
async fn sent_whole(chunks: usize) -> rig_http::Response<rig_http::LazyBody<Bytes>> {
    let url = serve_chunks(chunks).await;
    let transport = Transport::new(reqwest::Client::new(), "lease-1", "fake");
    let request = rig_http::Request::builder()
        .method(Method::POST)
        .uri(url)
        .body(Bytes::new())
        .unwrap();
    transport.http().send(request).await.unwrap()
}

/// Shared error identity survives both SDK HTTP response readers.
fn is_oversize(error: &(dyn StdError + 'static)) -> bool {
    std::iter::successors(Some(error), |&source| source.source())
        .filter_map(|source| source.downcast_ref::<afd_inference::Error>())
        .any(afd_inference::Error::is_oversize)
}

// A `custom:` endpoint's URL is the tenant's to spell; the retry line is the
// operator's to read, so it carries the host and nothing the URL may hide.
#[test]
fn a_custom_endpoint_is_logged_by_its_host_alone() {
    assert_eq!(
        &*logged("custom:https://user:s3cret@vllm.corp/v1?key=k3y"),
        "custom:vllm.corp"
    );
    assert_eq!(&*logged("anthropic"), "anthropic");
}

// rig's buffered seam is the one path that reads a reply whole, so the cap
// has to hold there too, and come out as the same refusal the streamed read
// gives, where the turn can find it.
#[tokio::test]
async fn a_whole_reply_past_the_cap_is_refused_at_rigs_buffered_seam() {
    let sent = sent_whole(REPLY_MAX_BYTES / SERVED_CHUNK + 1).await;

    let refused = sent.into_body().await.unwrap_err();

    assert!(
        matches!(&refused, rig_http::Error::Instance(failure) if is_oversize(&**failure)),
        "{refused:?}"
    );
}

#[tokio::test]
async fn a_whole_reply_under_the_cap_is_read_whole_at_rigs_buffered_seam() {
    let sent = sent_whole(2).await;

    let body = sent.into_body().await.unwrap();

    assert_eq!(body.len(), 2 * SERVED_CHUNK);
}

#[tokio::test]
async fn a_multipart_send_is_refused_since_no_wire_uploads_a_file() {
    let transport = Transport::new(reqwest::Client::new(), "lease-1", "fake");
    let request = rig_http::Request::builder()
        .method(Method::POST)
        .uri("http://127.0.0.1:9/v1/files")
        .body(MultipartForm::new())
        .unwrap();

    let refused = transport
        .http()
        .send_multipart::<Bytes>(request)
        .await
        .map(drop)
        .unwrap_err();

    assert!(
        matches!(&refused, rig_http::Error::Instance(failure) if failure.to_string().contains("multipart")),
        "{refused:?}"
    );
}

#[test]
fn a_transport_prints_whose_sends_it_makes_and_not_its_client() {
    let transport = Transport::new(reqwest::Client::new(), "lease-1", "fake");

    assert_eq!(
        format!("{transport:?}"),
        r#"Transport { lease_id: "lease-1", provider: "fake", .. }"#
    );
}
