//! Real socket reads exercise transport bounds; model endpoint authorization is separate.
#![expect(
    clippy::expect_used,
    reason = "socket fixture failures must fail loudly"
)]

use super::Network;
use crate::{ProviderRequest, Transport};
use afd_wire::authoring::ENVELOPE_BYTES_MAX;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

#[tokio::test]
async fn bounded_responses_keep_status_and_bytes_and_refuse_overflow() {
    for length in [3, ENVELOPE_BYTES_MAX, ENVELOPE_BYTES_MAX + 1] {
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
            timeout_ms: 5000,
        };
        let response = Network::new().expect("direct client").send(request).await;
        if length <= ENVELOPE_BYTES_MAX {
            let response = response.expect("bounded provider response");
            assert_eq!(response.status, 429);
            assert_eq!(response.body, vec![b'x'; length]);
        } else {
            let error = response.expect_err("unbounded response refused");
            assert_eq!(error.code(), afd_core::error_code::AUTHORING_UNKNOWN);
            assert!(!format!("{error:?}").contains("inference"));
        }
        server.await.expect("server stopped");
    }
}

#[tokio::test]
async fn a_relative_request_is_refused_before_any_socket_can_open() {
    let request = ProviderRequest {
        request: http::Request::new(Vec::new()),
        timeout_ms: 1,
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
