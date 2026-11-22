#![expect(clippy::unwrap_used, reason = "fixture preconditions must fail loudly")]

use super::*;
use futures_util::{StreamExt as _, stream};
use std::sync::Mutex;

/// The documented 32 MiB bound a test sets: the runner frame an embedder may
/// pass, independent of any constant of this crate.
const REPLY_MAX_BYTES: usize = 33_554_432;

struct Physical(Mutex<Option<Body>>);

impl Send for Physical {
    fn send(&self, _: Request<Bytes>) -> Sent {
        let body = self.0.lock().unwrap().take().unwrap();
        Box::pin(async move {
            Ok(Response::builder()
                .status(429)
                .header(http::header::RETRY_AFTER, "7")
                .body(body)
                .unwrap())
        })
    }
}

fn transport(chunks: Vec<Result<Bytes, SendError>>) -> HttpTransport {
    let body = Box::pin(stream::iter(chunks));
    HttpTransport::new(Arc::new(Physical(Mutex::new(Some(body))))).with_bound(Some(REPLY_MAX_BYTES))
}

fn request() -> Request<Bytes> {
    Request::new(Bytes::new())
}

fn oversized(error: &rig_http::Error) -> bool {
    let first: &(dyn std::error::Error + 'static) = error;
    std::iter::successors(Some(first), |&source| source.source())
        .filter_map(|source| source.downcast_ref::<Error>())
        .any(Error::is_reply_too_large)
}

#[tokio::test]
async fn response_headers_arrive_without_collecting_the_body() {
    let transport = transport(vec![Ok(Bytes::from_static(b"private body"))]);
    let mut response = transport.send_streaming(request()).await.unwrap();
    assert_eq!(response.status(), 429);
    assert_eq!(
        response.headers().get(http::header::RETRY_AFTER).unwrap(),
        "7"
    );
    assert_eq!(transport.status(), 429);
    assert!(!format!("{transport:?}").contains("private"));
    assert_eq!(
        response.body_mut().next().await.unwrap().unwrap(),
        "private body"
    );
}

#[tokio::test]
async fn exact_wire_limit_is_accepted_by_the_buffered_sdk_seam() {
    let transport = transport(vec![Ok(Bytes::from(vec![b'x'; REPLY_MAX_BYTES]))]);
    let response = transport.send(request()).await.unwrap();
    assert_eq!(response.into_body().await.unwrap().len(), REPLY_MAX_BYTES);
}

#[tokio::test]
async fn the_first_byte_past_the_limit_ends_the_stream_with_the_shared_error() {
    let transport = transport(vec![
        Ok(Bytes::from(vec![b'x'; REPLY_MAX_BYTES])),
        Ok(Bytes::from_static(b"x")),
        Ok(Bytes::from_static(b"never published")),
    ]);
    let mut response = transport
        .send_streaming(request())
        .await
        .unwrap()
        .into_body();
    assert_eq!(
        response.next().await.unwrap().unwrap().len(),
        REPLY_MAX_BYTES
    );
    let refused = response.next().await.unwrap().unwrap_err();
    assert!(oversized(&refused), "{refused:?}");
    assert!(
        response.next().await.is_none(),
        "no reads after the bound failure"
    );
}

#[tokio::test]
async fn buffered_and_streaming_sdk_seams_enforce_the_same_limit() {
    let transport = transport(vec![Ok(Bytes::from(vec![b'x'; REPLY_MAX_BYTES + 1]))]);
    let response = transport.send(request()).await.unwrap();
    assert!(oversized(&response.into_body().await.unwrap_err()));
}

#[tokio::test]
async fn body_disconnect_preserves_its_source_and_is_never_classified_as_oversize() {
    let transport = transport(vec![
        Ok(Bytes::from_static(b"part")),
        Err(SendError::Failed(Box::new(std::io::Error::other(
            "fixture reset",
        )))),
    ]);
    let mut body = transport
        .send_streaming(request())
        .await
        .unwrap()
        .into_body();
    assert_eq!(body.next().await.unwrap().unwrap(), "part");
    let failure = body.next().await.unwrap().unwrap_err();
    assert!(!oversized(&failure));
    assert!(failure.source().is_some());
    assert!(body.next().await.is_none());
}

#[tokio::test]
async fn multipart_is_refused_without_opening_the_physical_sender() {
    let transport = transport(Vec::new());
    let error = transport
        .send_multipart(Request::new(MultipartForm::new()))
        .await
        .err()
        .unwrap();
    assert!(error.to_string().contains("multipart"));
    assert_eq!(transport.status(), 0);
}

/// The bytes one chunk of a served reply carries.
#[cfg(feature = "reqwest")]
const SERVED_CHUNK: usize = 64 * 1024;

/// A reply of `chunks` served chunks from a loopback server, answered
/// through the default transport and rig's buffered seam, its body still to
/// be read.
#[cfg(feature = "reqwest")]
async fn sent_whole(chunks: usize) -> Response<LazyBody> {
    let app = axum::Router::new().fallback(async move || {
        let chunk = Bytes::from(vec![b'x'; SERVED_CHUNK]);
        let body = std::iter::repeat_n(chunk, chunks).map(Ok::<_, std::convert::Infallible>);
        axum::body::Body::from_stream(stream::iter(body))
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let transport =
        HttpTransport::new(Arc::new(Reqwest::new().unwrap())).with_bound(Some(REPLY_MAX_BYTES));
    let request = Request::builder()
        .method(http::Method::POST)
        .uri(format!("http://{address}/v1/chat/completions"))
        .body(Bytes::new())
        .unwrap();
    transport.send(request).await.unwrap()
}

// rig's buffered seam is the one path that reads a reply whole, so the bound
// has to hold there too, and come out as the same refusal the streamed read
// gives, where the turn can find it.
#[cfg(feature = "reqwest")]
#[tokio::test]
async fn a_whole_reply_past_the_cap_is_refused_at_rigs_buffered_seam() {
    let sent = sent_whole(REPLY_MAX_BYTES / SERVED_CHUNK + 1).await;

    let refused = sent.into_body().await.unwrap_err();

    assert!(
        matches!(&refused, rig_http::Error::Permanent(_)) && oversized(&refused),
        "{refused:?}"
    );
}

#[cfg(feature = "reqwest")]
#[tokio::test]
async fn a_whole_reply_under_the_cap_is_read_whole_at_rigs_buffered_seam() {
    let sent = sent_whole(2).await;

    let body = sent.into_body().await.unwrap();

    assert_eq!(body.len(), 2 * SERVED_CHUNK);
}

#[test]
fn each_send_failure_says_what_failed_and_keeps_its_cause() {
    let cause = || -> Cause { Box::new(std::io::Error::other("fixture cause")) };
    for (failure, text) in [
        (
            SendError::Refused(cause()),
            "the transport refused the destination",
        ),
        (
            SendError::Unanswered(cause()),
            "the request was not answered",
        ),
        (SendError::Failed(cause()), "the transport failed"),
    ] {
        assert_eq!(failure.to_string(), text);
        assert!(
            failure
                .source()
                .is_some_and(|source| source.to_string() == "fixture cause")
        );
    }
}

#[cfg(feature = "reqwest")]
#[test]
fn the_default_transport_prints_nothing_of_its_client() {
    let transport = Reqwest::new().unwrap();

    assert_eq!(format!("{transport:?}"), "Reqwest { .. }");
}

// Value: a request the client cannot even build is no unanswered send, so
// nothing retries it.
#[cfg(feature = "reqwest")]
#[tokio::test]
async fn a_request_the_client_cannot_build_fails_without_being_unanswered() {
    let relative = Request::builder()
        .uri("/v1/chat/completions")
        .body(Bytes::new())
        .unwrap();

    let failure = Reqwest::new().unwrap().send(relative).await.err().unwrap();

    assert!(matches!(failure, SendError::Failed(_)), "{failure:?}");
    assert!(!failure.is_unanswered());
    assert!(!failure.is_refused());
}

// Value: a reply cut short mid-body reaches the reader as the transport's
// failure, with the client's own error beneath it.
#[cfg(feature = "reqwest")]
#[tokio::test]
async fn a_reply_cut_mid_body_ends_its_stream_as_a_transport_failure() {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let served = tokio::spawn(async move {
        let (mut stream, _peer) = listener.accept().await.unwrap();
        let mut asked = Vec::new();
        while !asked.ends_with(b"\r\n\r\n") {
            let mut byte = [0; 1];
            stream.read_exact(&mut byte).await.unwrap();
            asked.extend_from_slice(&byte);
        }
        stream
            .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 64\r\n\r\npartial")
            .await
            .unwrap();
        stream.shutdown().await.unwrap();
    });
    let request = Request::builder()
        .uri(format!("http://{address}/v1/chat/completions"))
        .body(Bytes::new())
        .unwrap();

    let response = Reqwest::new().unwrap().send(request).await.unwrap();
    let status = response.status();
    let read: Vec<_> = response.into_body().collect().await;
    served.await.unwrap();

    assert_eq!(status, http::StatusCode::OK);
    let failures: Vec<_> = read.into_iter().filter_map(Result::err).collect();
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert!(failures.iter().all(|failure| {
        matches!(failure, SendError::Failed(_))
            && failure
                .source()
                .is_some_and(<dyn std::error::Error>::is::<::reqwest::Error>)
    }));
}
