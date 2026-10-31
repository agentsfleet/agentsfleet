#![expect(
    clippy::unwrap_used,
    reason = "test target: a fixture that cannot be built is a broken test"
)]
#![expect(
    clippy::disallowed_methods,
    reason = "tests feed a raw builder to the guard under test, or use a plain client so a loopback server answers; afd_identity::direct is a daemon crate the runner does not depend on"
)]

use std::io::{Read as _, Write as _};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use reqwest::header::HeaderMap;
use reqwest::{Client, Method, Url};

use super::{BlockedAddress, Capped, Network, guarded_lookup, origin_of};
use crate::error::raise;
use crate::fixture::shown;
use crate::testing::FakeResolver;
use crate::transport::{Outbound, Transport};

/// How long a read that should return at once is given before the test fails.
const PROMPTLY: Duration = Duration::from_secs(5);
/// A loopback listener on whatever port the system hands out.
const ANY_LOOPBACK_PORT: &str = "127.0.0.1:0";
/// A name the resolver answers with loopback, one it answers with a public
/// address, and one it has never heard of.
const INSIDE: &str = "inside.example";
const PUBLIC: &str = "public.example";
const NOWHERE: &str = "nowhere.example";
const PUBLIC_ADDRESS: IpAddr = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1));

/// A resolver answering [`INSIDE`] with loopback and [`PUBLIC`] with a public
/// address, so the guard is driven with no DNS.
fn resolver() -> Arc<FakeResolver> {
    Arc::new(FakeResolver::answering(&[
        (INSIDE, &[IpAddr::V4(Ipv4Addr::LOCALHOST)]),
        (PUBLIC, &[PUBLIC_ADDRESS]),
    ]))
}

/// The guarded client, resolving through [`resolver`].
fn network() -> Network {
    Network::build(Client::builder(), resolver()).unwrap()
}

/// A plain-HTTP server on a loopback port that reads one request's head,
/// answers it with `reply`, and holds the connection open until the client
/// closes it.
fn answering_and_holding(reply: &'static [u8]) -> SocketAddr {
    let listener = TcpListener::bind(ANY_LOOPBACK_PORT).unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut head = Vec::new();
        let mut byte = [0_u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            stream.read_exact(&mut byte).unwrap();
            head.extend_from_slice(&byte);
        }
        stream.write_all(reply).unwrap();
        let _closed = std::io::copy(&mut stream, &mut std::io::sink());
    });
    address
}

#[test]
fn should_keep_a_body_whole_under_the_cap_and_cut_it_past() {
    let mut whole = Capped::default();
    assert!(whole.push(b"abc", 6));
    assert!(whole.push(b"def", 6));
    assert_eq!(
        (whole.bytes.as_slice(), whole.truncated),
        (b"abcdef".as_slice(), false)
    );

    let mut cut = Capped::default();
    assert!(cut.push(b"abcd", 6));
    assert!(!cut.push(b"efgh", 6));
    assert_eq!(
        (cut.bytes.as_slice(), cut.truncated),
        (b"abcdef".as_slice(), true)
    );
}

#[test]
fn should_show_a_redirect_as_its_origin_alone() {
    let request =
        Url::parse("https://api.github.com/repos/acme/widgets/actions/jobs/7/logs").unwrap();

    assert_eq!(
        origin_of(&request, "https://logs.example.net/job/7?sig=secret").as_deref(),
        Some("https://logs.example.net")
    );
    assert_eq!(
        origin_of(&request, "/elsewhere?token=1").as_deref(),
        Some("https://api.github.com")
    );
}

#[tokio::test]
async fn should_refuse_a_name_that_resolves_to_loopback() {
    let refused = guarded_lookup(&*resolver(), INSIDE).await.err();

    assert!(refused.is_some_and(|refusal| refusal.is::<BlockedAddress>()));
}

#[tokio::test]
async fn should_answer_address_not_allowed_when_the_resolver_refuses_the_name() {
    let outbound = Outbound {
        method: Method::GET,
        url: Url::parse(&format!("https://{INSIDE}:9/")).unwrap(),
        headers: HeaderMap::new(),
        body: None,
        discard_redirect_body: false,
    };

    let refused = network().send(outbound).await.err().as_ref().map(shown);

    assert_eq!(refused, Some(shown(&raise::address_not_allowed(INSIDE))));
}

#[tokio::test]
async fn should_answer_unreachable_when_the_name_does_not_resolve() {
    let outbound = Outbound {
        method: Method::GET,
        url: Url::parse(&format!("https://{NOWHERE}/")).unwrap(),
        headers: HeaderMap::new(),
        body: None,
        discard_redirect_body: false,
    };

    let refused = network().send(outbound).await.err().as_ref().map(shown);

    assert_eq!(
        refused,
        Some(shown(&raise::upstream_unreachable(
            NOWHERE,
            super::NOT_CONNECTED
        )))
    );
}

// A host that accepts and never answers: the handshake waits on a reply that
// never comes, so only the timer ends the request, and the paused clock runs
// it out at once.
#[tokio::test(start_paused = true)]
async fn should_answer_timed_out_when_the_host_never_answers() {
    let listener = tokio::net::TcpListener::bind(ANY_LOOPBACK_PORT)
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _held = listener.accept().await;
        std::future::pending::<()>().await;
    });
    let outbound = Outbound {
        method: Method::GET,
        url: Url::parse(&format!("https://{address}/")).unwrap(),
        headers: HeaderMap::new(),
        body: None,
        discard_redirect_body: false,
    };

    let refused = network().send(outbound).await.err().as_ref().map(shown);

    let host = address.ip().to_string();
    let timed_out = raise::upstream_unreachable(&host, super::TIMED_OUT);
    assert_eq!(refused, Some(shown(&timed_out)));
}

// A chunked body whose last chunk never comes: only the cap ends the read, so
// a read that went on past it would wait here until the test gave up.
#[tokio::test]
async fn should_stop_reading_a_body_once_it_passes_the_cap() {
    let address = answering_and_holding(
        b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n8\r\nabcdefgh\r\n",
    );
    let response = Client::new()
        .get(format!("http://{address}/"))
        .send()
        .await
        .unwrap();

    let read = tokio::time::timeout(PROMPTLY, Capped::read(response, 6)).await;

    let read = read.unwrap().unwrap();
    assert_eq!(
        (read.bytes.as_slice(), read.truncated),
        (b"abcdef".as_slice(), true)
    );
}

// The client here is plain, without the guard or `https_only`, so the loopback
// server answers: what is under test is how a body that is not UTF-8 is handed
// to the tool, after the guard has already let the request go.
#[tokio::test]
async fn should_hand_back_a_body_that_is_not_utf8_with_its_bad_bytes_replaced() {
    let address = answering_and_holding(b"HTTP/1.1 200 OK\r\ncontent-length: 3\r\n\r\nok\xff");
    let network = Network {
        client: Client::new(),
    };
    let outbound = Outbound {
        method: Method::GET,
        url: Url::parse(&format!("http://{address}/")).unwrap(),
        headers: HeaderMap::new(),
        body: None,
        discard_redirect_body: false,
    };

    let inbound = network.send(outbound).await.unwrap();

    assert_eq!(
        (inbound.status, inbound.body.as_str(), inbound.truncated),
        (200, "ok\u{fffd}", false)
    );
    assert!(inbound.lossy);
}

#[tokio::test]
async fn should_answer_every_address_of_a_host_none_of_whose_addresses_is_blocked() {
    let resolved = guarded_lookup(&*resolver(), PUBLIC).await.unwrap();

    assert_eq!(
        resolved.collect::<Vec<_>>(),
        [SocketAddr::new(PUBLIC_ADDRESS, 0)]
    );
}

#[tokio::test]
async fn should_discard_redirect_bodies_before_waiting_for_them() {
    let address = answering_and_holding(
        b"HTTP/1.1 302 Found\r\nlocation: https://logs.example/download?sig=private-target\r\ntransfer-encoding: chunked\r\n\r\n",
    );
    let network = Network {
        client: Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .build()
            .unwrap(),
    };
    let outbound = Outbound {
        method: Method::GET,
        url: Url::parse(&format!("http://{address}/")).unwrap(),
        headers: HeaderMap::new(),
        body: None,
        discard_redirect_body: false,
    };
    let response = tokio::time::timeout(PROMPTLY, network.send(outbound.for_redirect_read()))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(response.status, 302);
    assert!(response.body.is_empty());
    assert_eq!(response.location.as_deref(), Some("https://logs.example"));
    assert!(!format!("{response:?}").contains("private-target"));
}

/// Observe actual HTTP/2 request headers, then refuse that stream.
async fn resetting_server() -> (SocketAddr, Arc<AtomicUsize>, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind(ANY_LOOPBACK_PORT)
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let seen = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&seen);
    let task = tokio::spawn(async move {
        let mut connections = tokio::task::JoinSet::new();
        while let Ok((stream, _peer)) = listener.accept().await {
            let count = Arc::clone(&count);
            connections.spawn(async move {
                let mut connection = h2::server::handshake(stream).await.unwrap();
                while let Some(request) = connection.accept().await {
                    let (_headers, mut reply) = request.unwrap();
                    count.fetch_add(1, Ordering::SeqCst);
                    reply.send_reset(h2::Reason::REFUSED_STREAM);
                }
            });
        }
    });
    (address, seen, task)
}

#[tokio::test]
async fn should_disable_implicit_tool_request_retries() {
    for method in [Method::GET, Method::POST] {
        for guarded_tool in [true, false] {
            let (address, seen, task) = resetting_server().await;
            let builder = Client::builder().http2_prior_knowledge();
            let builder = if guarded_tool {
                super::tool_client(builder, resolver())
            } else {
                // Inference uses this shared guard and retains reqwest's default
                // two retries of a remote REFUSED_STREAM protocol failure.
                super::guarded_by(builder, resolver())
            };
            // Only HTTPS is relaxed for the local wire fixture. The production
            // tool configuration, including its retry override, is unchanged.
            let network = Network {
                client: builder.https_only(false).build().unwrap(),
            };
            let outbound = Outbound {
                method: method.clone(),
                url: Url::parse(&format!("http://{address}/reset")).unwrap(),
                headers: HeaderMap::new(),
                body: None,
                discard_redirect_body: false,
            };
            let refused = network.send(outbound).await.err().unwrap();
            assert_eq!(refused.refusal(), Some(crate::Refusal::UpstreamUnreachable));
            assert_eq!(
                seen.load(Ordering::SeqCst),
                if guarded_tool { 1 } else { 3 }
            );
            task.abort();
        }
    }
}
