//! The scheduler client honours an environment proxy.
//!
//! A test binary of its own because it is the one test that WRITES the process
//! environment: `std::env::set_var` is `unsafe` in edition 2024, sound only
//! while no other thread reads the environment, and `daemon_suite` runs its
//! tests in parallel inside one binary. This binary holds one test, which sets
//! the variables before it starts a runtime or a thread.
//!
//! The mirror of `afd_identity`'s `direct_client`: that suite proves a direct
//! client ignores the proxy, this one proves the scheduler's client goes
//! through it, as `credentials::schedule_client` promises.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::sync::oneshot;

/// Every variable reqwest reads a proxy from.
const PROXY_VARIABLES: [&str; 6] = [
    "HTTP_PROXY",
    "http_proxy",
    "HTTPS_PROXY",
    "https_proxy",
    "ALL_PROXY",
    "all_proxy",
];

/// The variables that exempt hosts from every proxy above.
const BYPASS_VARIABLES: [&str; 2] = ["NO_PROXY", "no_proxy"];

/// A scheduler URL on a host that cannot resolve: reached only through the
/// proxy, never directly. Plain `http` so the proxy is handed the request in
/// absolute form rather than a `CONNECT`.
const SCHEDULER_URL: &str = "http://scheduler.invalid/v2/schedules";

/// The request line a proxy receives for [`SCHEDULER_URL`].
const EXPECTED_REQUEST: &str = "GET http://scheduler.invalid/v2/schedules HTTP/1.1";

/// What the fake proxy answers on the scheduler's behalf.
const ANSWER: &[u8] = b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok";

/// What the fake proxy's request buffer holds.
const REQUEST_BUFFER_BYTES: usize = 1024;

/// Binds the fake proxy's loopback port, synchronously and before any runtime
/// exists, so its address can be written into the environment first.
fn bind_proxy() -> (std::net::TcpListener, String) {
    let listener =
        std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).expect("a loopback port");
    listener
        .set_nonblocking(true)
        .expect("a listener tokio can adopt");
    let url = format!("http://{}", listener.local_addr().expect("a bound address"));
    (listener, url)
}

/// Serves one connection as a fake proxy: hands the first request line it
/// reads to the returned receiver, then answers [`ANSWER`].
fn serve_proxy(listener: std::net::TcpListener) -> oneshot::Receiver<String> {
    let listener = tokio::net::TcpListener::from_std(listener).expect("an adopted listener");
    let (seen, received) = oneshot::channel();
    tokio::spawn(async move {
        let Ok((mut socket, _peer)) = listener.accept().await else {
            return;
        };
        let mut request = vec![0; REQUEST_BUFFER_BYTES];
        let read = socket.read(&mut request).await.unwrap_or(0);
        let text = String::from_utf8_lossy(request.get(..read).unwrap_or_default());
        let line = text.lines().next().unwrap_or_default().to_owned();
        let _sent = seen.send(line);
        let _written = socket.write_all(ANSWER).await;
    });
    received
}

/// With every proxy variable naming the fake proxy, a call on the scheduler's
/// client reaches the proxy in absolute form and comes back with its answer: a
/// host that cannot resolve is reachable only that way.
#[test]
fn the_scheduler_client_goes_through_an_environment_proxy() {
    let (listener, proxy) = bind_proxy();
    for variable in PROXY_VARIABLES {
        // SAFETY: this binary's one test, before any runtime or thread exists,
        // so nothing reads the environment while it is written.
        unsafe { std::env::set_var(variable, &proxy) };
    }
    for variable in BYPASS_VARIABLES {
        // SAFETY: as above. A host's own bypass list could otherwise exempt
        // the very host this test asks for.
        unsafe { std::env::remove_var(variable) };
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime");

    runtime.block_on(async {
        let mut received = serve_proxy(listener);
        let client = agentsfleetd::credentials::proxied_schedule_client().expect("a client");

        let response = client.get(SCHEDULER_URL).send().await;
        // The proxy hands its line over BEFORE it answers, so a call that went
        // through it has already left the line here; one that did not leaves
        // nothing, and the assertion names the request that never came.
        let line = received.try_recv().unwrap_or_default();

        assert_eq!(
            line, EXPECTED_REQUEST,
            "the call did not go through the proxy"
        );
        let status = response.map(|answer| answer.status()).ok();
        assert_eq!(status, Some(reqwest::StatusCode::OK));
    });
}
