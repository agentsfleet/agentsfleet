#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::convert::Infallible;
use std::error::Error as StdError;

use bytes::Bytes;
use futures_util::{StreamExt as _, stream};
use http::Method;
use rig_core::http_client::{self as rig_http, HttpClientExt as _};

use super::{Oversize, REPLY_MAX_BYTES, Transport, Unread, capped, whole};

/// The cap every direct read here is under.
const CAP: usize = 8;
/// Why a cut reply's body ended: the transport's own failure, never the cap's.
const DROPPED: &str = "the connection dropped";
/// The bytes one chunk of a served reply carries.
const SERVED_CHUNK: usize = 64 * 1024;

/// A reply whose body arrives as one chunk of each length in `chunks`.
fn reply(chunks: &[usize]) -> reqwest::Response {
    let chunks: Vec<_> = (chunks.iter())
        .map(|&len| Ok::<_, std::io::Error>(Bytes::from(vec![b'x'; len])))
        .collect();
    let body = reqwest::Body::wrap_stream(stream::iter(chunks));
    reqwest::Response::from(http::Response::new(body))
}

/// A reply whose body arrives as one chunk of each length in `chunks`, then
/// fails as a connection that dropped before the body ended.
fn cut_reply(chunks: &[usize]) -> reqwest::Response {
    let chunks: Vec<_> = (chunks.iter())
        .map(|&len| Ok(Bytes::from(vec![b'x'; len])))
        .chain([Err(std::io::Error::other(DROPPED))])
        .collect();
    let body = reqwest::Body::wrap_stream(stream::iter(chunks));
    reqwest::Response::from(http::Response::new(body))
}

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
    transport.send(request).await.unwrap()
}

/// Whether `error` is the read's refusal at the cap.
fn is_oversize(error: &Unread) -> bool {
    error.is::<Oversize>()
}

/// Whether `error`, or a cause of it, is the dropped connection.
fn is_dropped(error: &Unread) -> bool {
    let first: &(dyn StdError + 'static) = &**error;
    std::iter::successors(Some(first), |&error| error.source())
        .any(|error| error.to_string() == DROPPED)
}

#[tokio::test]
async fn a_reply_of_exactly_the_cap_is_read_whole() {
    let body = whole(reply(&[4, 4]), CAP).await.unwrap();

    assert_eq!(body.len(), CAP);
}

#[tokio::test]
async fn a_reply_one_byte_past_the_cap_is_refused_whole() {
    let refused = whole(reply(&[4, 4, 1]), CAP).await.unwrap_err();

    assert!(is_oversize(&refused), "{refused:?}");
}

#[tokio::test]
async fn a_stream_passes_its_chunks_until_the_one_that_crosses_the_cap() {
    let read: Vec<_> = capped(reply(&[4, 4, 1, 4]), CAP).collect().await;

    let lengths: Vec<_> = read
        .iter()
        .map(|chunk| chunk.as_ref().map(Bytes::len).ok())
        .collect();
    assert_eq!(lengths[..2], [Some(4), Some(4)]);
    assert!(read[2].as_ref().is_err_and(is_oversize), "{read:?}");
}

// The turn reads the failure's cause to decide whether to open the turn again:
// a dropped connection may pass on a second opening, a reply at the cap never
// will, so the one must never be dressed as the other.
#[tokio::test]
async fn a_body_that_fails_mid_stream_passes_the_transports_own_failure_through() {
    let read: Vec<_> = capped(cut_reply(&[4]), CAP).collect().await;

    assert_eq!(read[0].as_ref().map(Bytes::len).ok(), Some(4));
    let failure = read[1].as_ref().unwrap_err();
    assert!(is_dropped(failure) && !is_oversize(failure), "{failure:?}");
    assert_eq!(read.len(), 2, "the read ends on the failure");
}

// rig's buffered seam is the one path that reads a reply whole, so the cap
// has to hold there too, and come out as the same refusal the streamed read
// gives, where the turn can find it.
#[tokio::test]
async fn a_whole_reply_past_the_cap_is_refused_at_rigs_buffered_seam() {
    let sent = sent_whole(REPLY_MAX_BYTES / SERVED_CHUNK + 1).await;

    let refused = sent.into_body().await.unwrap_err();

    assert!(
        matches!(&refused, rig_http::Error::Instance(failure) if is_oversize(failure)),
        "{refused:?}"
    );
}
