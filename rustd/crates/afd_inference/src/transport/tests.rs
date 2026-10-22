#![expect(clippy::unwrap_used, reason = "fixture preconditions must fail loudly")]

use super::*;
use futures_util::{StreamExt as _, stream};
use std::{error::Error as _, sync::Mutex};

struct Physical(Mutex<Option<rig_http::BoxedStream>>);

impl SendRequest for Physical {
    fn send(&self, _: Request<Bytes>) -> BoxFuture<'static, rig_http::Result<StreamingResponse>> {
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

fn transport(chunks: Vec<rig_http::Result<Bytes>>) -> HttpTransport {
    let body = Box::pin(stream::iter(chunks));
    HttpTransport::new(Arc::new(Physical(Mutex::new(Some(body)))))
}

fn request() -> Request<Bytes> {
    Request::new(Bytes::new())
}

fn oversized(error: &rig_http::Error) -> bool {
    let first: &(dyn std::error::Error + 'static) = error;
    std::iter::successors(Some(first), |&source| source.source())
        .filter_map(|source| source.downcast_ref::<Error>())
        .any(Error::is_oversize)
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
    // The documented 32 MiB limit is independent of the implementation's constant.
    const DOCUMENTED_BYTES: usize = 33_554_432;
    let transport = transport(vec![Ok(Bytes::from(vec![b'x'; DOCUMENTED_BYTES]))]);
    let response = transport.send::<_, Bytes>(request()).await.unwrap();
    assert_eq!(response.into_body().await.unwrap().len(), DOCUMENTED_BYTES);
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
    let response = transport.send::<_, Bytes>(request()).await.unwrap();
    assert!(oversized(&response.into_body().await.unwrap_err()));
}

#[tokio::test]
async fn body_disconnect_preserves_its_source_and_is_never_classified_as_oversize() {
    let transport = transport(vec![
        Ok(Bytes::from_static(b"part")),
        Err(rig_http::Error::instance(std::io::Error::other(
            "fixture reset",
        ))),
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
        .send_multipart::<Bytes>(Request::new(MultipartForm::new()))
        .await
        .err()
        .unwrap();
    assert!(error.to_string().contains("multipart"));
    assert_eq!(transport.status(), 0);
}
