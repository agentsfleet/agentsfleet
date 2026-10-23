//! Real socket reads exercise transport bounds; model endpoint authorization is separate.
#![expect(
    clippy::expect_used,
    reason = "socket fixture failures must fail loudly"
)]

use super::Network;
use crate::{ProviderRequest, Transport};
use futures_util::TryStreamExt as _;
use std::time::Duration;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

#[tokio::test]
async fn physical_responses_keep_status_and_stream_bytes_without_normalized_envelope_collection() {
    for length in [3, afd_wire::authoring::ENVELOPE_BYTES_MAX + 1] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("fixture listener");
        let address = listener.local_addr().expect("listener address");
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("one request");
            let mut request = [0; 4096];
            assert!(socket.read(&mut request).await.expect("request headers") > 0);
            socket.write_all(format!("HTTP/1.1 429 Too Many Requests\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n").as_bytes())
                .await.expect("response headers");
            // A refused response can close the connection before all bytes are written.
            let _closed = socket.write_all(&vec![b'x'; length]).await;
        });
        let request = ProviderRequest {
            request: http::Request::builder()
                .uri(format!("http://{address}/inference"))
                .body(Vec::new())
                .expect("fixture request"),
            timeout: Duration::from_secs(5),
        };
        let response = Network::new().expect("direct client").send(request).await;
        let response = response.expect("provider response head");
        assert_eq!(response.status, 429);
        let body = response
            .body
            .try_fold(Vec::new(), |mut all, bytes| async move {
                all.extend_from_slice(&bytes);
                Ok(all)
            })
            .await
            .expect("physical bytes");
        assert_eq!(body, vec![b'x'; length]);
        server.await.expect("server stopped");
    }
}

#[tokio::test]
async fn response_head_and_first_bytes_arrive_before_the_provider_finishes() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener");
    let address = listener.local_addr().expect("address");
    let (release, wait) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("request");
        let mut request = [0; 4096];
        assert!(socket.read(&mut request).await.expect("headers") > 0);
        socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\nfirst")
            .await
            .expect("first bytes");
        wait.await.expect("test received first bytes");
        socket.write_all(b" last").await.expect("last bytes");
    });
    let request = ProviderRequest {
        request: http::Request::builder()
            .uri(format!("http://{address}/model"))
            .body(Vec::new())
            .expect("request"),
        timeout: Duration::from_secs(5),
    };
    let mut response = tokio::time::timeout(
        Duration::from_secs(1),
        Network::new().expect("network").send(request),
    )
    .await
    .expect("head before finish")
    .expect("response");
    assert_eq!(response.status, 200);
    let first = tokio::time::timeout(Duration::from_secs(1), response.body.try_next())
        .await
        .expect("first chunk before finish")
        .expect("read")
        .expect("chunk");
    assert_eq!(first.as_ref(), b"first");
    assert!(!server.is_finished());
    release.send(()).expect("release provider");
    assert_eq!(
        response
            .body
            .try_next()
            .await
            .expect("last read")
            .expect("last chunk")
            .as_ref(),
        b" last"
    );
    assert!(response.body.try_next().await.expect("end").is_none());
    server.await.expect("provider joined");
}

#[tokio::test]
async fn a_relative_request_is_refused_before_any_socket_can_open() {
    let request = ProviderRequest {
        request: http::Request::new(Vec::new()),
        timeout: Duration::from_millis(1),
    };
    let error = Network::new()
        .expect("direct client")
        .send(request)
        .await
        .expect_err("relative HTTP URI refused");
    assert_eq!(error.code(), afd_core::error_code::AUTHORING_UNKNOWN);
    let source = std::error::Error::source(&error).expect("typed transport cause");
    assert!(source.downcast_ref::<reqwest::Error>().is_some());
}
