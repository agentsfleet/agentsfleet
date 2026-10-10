//! A direct client ignores an environment proxy.
//!
//! A test binary of its own because it is the one test that WRITES the process
//! environment: `std::env::set_var` is `unsafe` in edition 2024, sound only
//! while no other thread reads the environment, and every other suite here
//! runs its tests in parallel inside one binary. This binary holds one test,
//! which sets the variables before it starts a runtime or a thread.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::Duration;

use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

/// How long a request waits before giving up: short, because the control
/// request is meant to fail against a closed port.
const TIMEOUT: Duration = Duration::from_secs(5);
/// A loopback port nothing listens on; port 9 is `discard`, never served here.
const DEAD_PROXY: &str = "http://127.0.0.1:9";
/// Every variable reqwest reads a proxy from.
const PROXY_VARIABLES: [&str; 6] = [
    "HTTP_PROXY",
    "http_proxy",
    "HTTPS_PROXY",
    "https_proxy",
    "ALL_PROXY",
    "all_proxy",
];
/// What the loopback endpoint answers.
const ANSWER: &[u8] = b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok";
/// What the endpoint's request buffer holds.
const REQUEST_BUFFER_BYTES: usize = 1024;

/// Answers every connection on a loopback port with [`ANSWER`].
async fn serve() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a loopback port");
    let url = format!(
        "http://{}/",
        listener.local_addr().expect("a bound address")
    );
    tokio::spawn(async move {
        while let Ok((mut socket, _peer)) = listener.accept().await {
            let mut request = vec![0; REQUEST_BUFFER_BYTES];
            let _read = socket.read(&mut request).await;
            let _written = socket.write_all(ANSWER).await;
        }
    });
    url
}

/// With every proxy variable naming a dead port, the direct client still
/// reaches the endpoint, and a client built the default way does not: the
/// control proves the variables are read at all.
#[test]
fn a_direct_client_ignores_an_environment_proxy() {
    for variable in PROXY_VARIABLES {
        // SAFETY: this binary's one test, before any runtime or thread exists,
        // so nothing reads the environment while it is written.
        unsafe { std::env::set_var(variable, DEAD_PROXY) };
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime");

    runtime.block_on(async {
        let url = serve().await;
        let direct = afd_identity::direct(TIMEOUT).build().expect("a client");
        let proxied = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .build()
            .expect("a client");

        let reached = direct.get(&url).send().await.expect("the endpoint answers");
        let rerouted = proxied.get(&url).send().await;

        assert_eq!(reached.status(), reqwest::StatusCode::OK);
        assert!(rerouted.is_err(), "the control went through the dead proxy");
    });
}
